use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use gnort::instrument::TimingCount;
use nockapp::driver::{NockAppHandle, PokeResult};
use nockapp::noun::slab::NounSlab;
use nockapp::wire::WireRepr;
use nockchain_types::tx_engine::{v0, v1};
use nockvm::noun::SIG;
use noun_serde::{NounDecode, NounEncode};
use tokio::sync::RwLock;
use tokio::time::{self, Duration};
use tonic::transport::Server;
use tonic::{Request, Response, Status};
use tonic_reflection::server::Builder as ReflectionBuilder;
use tracing::{debug, info, warn};

use super::cache::{
    AddressBalanceCache, DEFAULT_PAGE_BYTES, DEFAULT_PAGE_SIZE, MAX_PAGE_BYTES, MAX_PAGE_SIZE,
};
use super::metrics::{init_metrics, NockchainGrpcApiMetrics};
use crate::error::{NockAppGrpcError, Result};
use crate::pb::common::v1::{Acknowledged, ErrorCode, ErrorStatus};
use crate::pb::public::v2::nockchain_service_server::{NockchainService, NockchainServiceServer};
use crate::pb::public::v2::*;
use crate::public_nockchain::v2::cache::{
    CachedBalanceEntryAddress, CachedBalanceEntryFirstName, FirstNameBalanceCache,
};
use crate::public_nockchain::v2::server::wallet_get_balance_request::Selector;
use crate::v2::pagination::{
    decode_cursor_address, decode_cursor_first_name, PageCursorAddress, PageCursorFirstName,
    PageKeyAddress, PageKeyFirstName,
};
use crate::wire_conversion::{create_grpc_wire, grpc_wire_to_nockapp};

const DEFAULT_HEAVIEST_CHAIN_REFRESH_INTERVAL: Duration = Duration::from_secs(60);

#[async_trait]
pub trait BalanceHandle: Send + Sync {
    async fn peek(
        &self,
        path: NounSlab,
    ) -> std::result::Result<Option<NounSlab>, nockapp::nockapp::error::NockAppError>;

    async fn poke(
        &self,
        wire: WireRepr,
        payload: NounSlab,
    ) -> std::result::Result<PokeResult, nockapp::nockapp::error::NockAppError>;
}

struct NockAppBalanceHandle(NockAppHandle);

#[async_trait]
impl BalanceHandle for NockAppBalanceHandle {
    async fn peek(
        &self,
        path: NounSlab,
    ) -> std::result::Result<Option<NounSlab>, nockapp::nockapp::error::NockAppError> {
        self.0.peek(path).await
    }

    async fn poke(
        &self,
        wire: WireRepr,
        payload: NounSlab,
    ) -> std::result::Result<PokeResult, nockapp::nockapp::error::NockAppError> {
        self.0.poke(wire, payload).await
    }
}

#[derive(Clone)]
pub struct PublicNockchainGrpcServer {
    handle: Arc<dyn BalanceHandle>,
    cache_by_address: AddressBalanceCache,
    cache_by_first_name: FirstNameBalanceCache,
    metrics: Arc<NockchainGrpcApiMetrics>,
    heaviest_chain: Arc<RwLock<Option<HeaviestChainSnapshot>>>,
}

#[derive(Clone)]
struct HeaviestChainSnapshot {
    height: v1::BlockHeight,
    block_id: v1::Hash,
    fetched_at: Instant,
}

impl PublicNockchainGrpcServer {
    pub fn new(handle: NockAppHandle) -> Self {
        Self {
            handle: Arc::new(NockAppBalanceHandle(handle)),
            cache_by_address: AddressBalanceCache::new(),
            cache_by_first_name: FirstNameBalanceCache::new(),
            metrics: init_metrics(),
            heaviest_chain: Arc::new(RwLock::new(None)),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_handle(handle: Arc<dyn BalanceHandle>) -> Self {
        Self {
            handle,
            cache_by_address: AddressBalanceCache::new(),
            cache_by_first_name: FirstNameBalanceCache::new(),
            metrics: init_metrics(),
            heaviest_chain: Arc::new(RwLock::new(None)),
        }
    }

    pub async fn serve(self, addr: SocketAddr) -> Result<()> {
        info!("Starting PublicNockchain gRPC server on {}", addr);
        let (health_reporter, health_service) = tonic_health::server::health_reporter();
        health_reporter
            .set_serving::<NockchainServiceServer<PublicNockchainGrpcServer>>()
            .await;
        let reflection_service_v1 = ReflectionBuilder::configure()
            .register_encoded_file_descriptor_set(nockapp_grpc_proto::pb::FILE_DESCRIPTOR_SET)
            .build_v1()
            .map_err(|e| {
                NockAppGrpcError::Internal(format!("Failed to build v1 reflection service: {}", e))
            })?;
        if let Err(err) = self.refresh_heaviest_chain().await {
            self.metrics.heaviest_chain_refresh_failure.increment();
            warn!("Failed to seed heaviest chain cache: {}", err);
        }
        self.start_heaviest_chain_refresh();
        let nockchain_api = NockchainServiceServer::new(self);
        Server::builder()
            .add_service(health_service)
            .add_service(reflection_service_v1)
            .add_service(nockchain_api)
            .serve(addr)
            .await
            .map_err(NockAppGrpcError::Transport)?;
        Ok(())
    }

    fn build_error_response<T>(&self, error: NockAppGrpcError) -> T
    where
        T: From<ErrorStatus>,
    {
        let error_status = ErrorStatus {
            code: match &error {
                NockAppGrpcError::PeekFailed => ErrorCode::PeekFailed as i32,
                NockAppGrpcError::PokeFailed => ErrorCode::PokeFailed as i32,
                NockAppGrpcError::Timeout => ErrorCode::Timeout as i32,
                NockAppGrpcError::InvalidRequest(_) => ErrorCode::InvalidRequest as i32,
                _ => ErrorCode::InternalError as i32,
            },
            message: error.to_string(),
            details: None,
        };
        T::from(error_status)
    }

    async fn peek_heaviest_chain(&self) -> Result<Option<(v1::BlockHeight, v1::Hash)>> {
        let metrics = &self.metrics;

        let mut path_slab = NounSlab::new();
        let tag = nockapp::utils::make_tas(&mut path_slab, "heaviest-chain").as_noun();
        let path_noun = nockvm::noun::T(&mut path_slab, &[tag, SIG]);
        path_slab.set_root(path_noun);

        let started = Instant::now();
        let peek_result = self.handle.peek(path_slab).await;
        metrics.heaviest_chain_peek.add_timing(&started.elapsed());

        let result = match peek_result {
            Ok(Some(result_slab)) => {
                let result_noun = unsafe { result_slab.root() };
                match <Option<Option<(v1::BlockHeight, v1::Hash)>>>::from_noun(&result_noun) {
                    Ok(opt) => Ok(opt.flatten()),
                    // Peek either returned [~ ~] or ~
                    Err(_) => Err(NockAppGrpcError::PeekReturnedNoData),
                }
            }
            Ok(None) => Err(NockAppGrpcError::PeekFailed),
            Err(e) => Err(NockAppGrpcError::from(e)),
        };

        result
    }

    fn start_heaviest_chain_refresh(&self) {
        let server = self.clone();
        tokio::spawn(async move {
            let mut interval = time::interval(DEFAULT_HEAVIEST_CHAIN_REFRESH_INTERVAL);
            loop {
                interval.tick().await;
                if let Err(err) = server.refresh_heaviest_chain().await {
                    server.metrics.heaviest_chain_refresh_failure.increment();
                    warn!("Failed to refresh heaviest chain cache: {}", err);
                }
            }
        });
    }

    async fn refresh_heaviest_chain(&self) -> Result<()> {
        match self.peek_heaviest_chain().await? {
            Some((height, block_id)) => {
                tracing::debug!("refreshed heaviest chain");
                let mut guard = self.heaviest_chain.write().await;
                let new_height_value = height.0 .0;
                let should_update = guard
                    .as_ref()
                    .map(|current| new_height_value >= current.height.0 .0)
                    .unwrap_or(true);

                if should_update {
                    let snapshot = HeaviestChainSnapshot {
                        height,
                        block_id,
                        fetched_at: Instant::now(),
                    };
                    *guard = Some(snapshot);
                    self.metrics.heaviest_chain_age_seconds.swap(0.0);
                } else if let Some(current) = guard.as_ref() {
                    warn!(
                        new_height = new_height_value,
                        cached_height = current.height.0 .0,
                        "Heaviest chain peek returned lower height than cache"
                    );
                }
            }
            None => {}
        }
        Ok(())
    }

    async fn cached_heaviest_chain(&self) -> Option<(v1::BlockHeight, v1::Hash)> {
        let guard = self.heaviest_chain.read().await;
        if let Some(snapshot) = guard.as_ref() {
            let age = snapshot.fetched_at.elapsed().as_secs_f64();
            self.metrics.heaviest_chain_age_seconds.swap(age);
            Some((snapshot.height.clone(), snapshot.block_id.clone()))
        } else {
            self.metrics.heaviest_chain_age_seconds.swap(-1.0);
            None
        }
    }
}

fn timed_return<T>(metric: &TimingCount, started: Instant, value: T) -> T {
    metric.add_timing(&started.elapsed());
    value
}

#[tonic::async_trait]
impl NockchainService for PublicNockchainGrpcServer {
    async fn wallet_get_balance(
        &self,
        request: Request<WalletGetBalanceRequest>,
    ) -> std::result::Result<Response<WalletGetBalanceResponse>, Status> {
        let req = request.into_inner();
        let request_start = Instant::now();
        let metrics = &self.metrics;

        let WalletGetBalanceRequest { selector, page, .. } = req;
        if selector.is_none() {
            self.metrics
                .balance_request_error_invalid_request_missing_selector
                .increment();
            let err = self.build_error_response::<ErrorStatus>(NockAppGrpcError::InvalidRequest(
                "selector is required".into(),
            ));
            return timed_return(
                &metrics.balance_update_error,
                request_start,
                Ok(Response::new(WalletGetBalanceResponse {
                    result: Some(wallet_get_balance_response::Result::Error(err)),
                })),
            );
        }

        let (client_page_items_limit, token, max_bytes) = if let Some(request) = page {
            (
                if request.client_page_items_limit == 0 {
                    DEFAULT_PAGE_SIZE
                } else {
                    std::cmp::min(request.client_page_items_limit as usize, MAX_PAGE_SIZE)
                },
                request.page_token,
                if request.max_bytes == 0 {
                    DEFAULT_PAGE_BYTES
                } else {
                    std::cmp::min(request.max_bytes, MAX_PAGE_BYTES)
                },
            )
        } else {
            self.metrics
                .balance_request_error_invalid_request_page_missing
                .increment();
            let err = self.build_error_response::<ErrorStatus>(NockAppGrpcError::InvalidRequest(
                "Page request is missing".into(),
            ));
            return timed_return(
                &metrics.balance_update_error,
                request_start,
                Ok(Response::new(WalletGetBalanceResponse {
                    result: Some(wallet_get_balance_response::Result::Error(err)),
                })),
            );
        };

        match selector.unwrap() {
            Selector::Address(address) => {
                let cursor: Option<PageCursorAddress> = if !token.is_empty() {
                    match decode_cursor_address(&token) {
                        Some(cur) => Some(cur),
                        None => {
                            self.metrics
                                .balance_request_error_invalid_request_token_invalid
                                .increment();
                            let err = ErrorStatus {
                                code: ErrorCode::InvalidRequest as i32,
                                message: "Invalid page token".into(),
                                details: None,
                            };
                            return timed_return(
                                &metrics.balance_update_error,
                                request_start,
                                Ok(Response::new(WalletGetBalanceResponse {
                                    result: Some(wallet_get_balance_response::Result::Error(err)),
                                })),
                            );
                        }
                    }
                } else {
                    None
                };

                if v1::SchnorrPubkey::from_base58(&address.key).is_err() {
                    self.metrics
                        .balance_request_error_invalid_request_address_format
                        .increment();
                    let err = self.build_error_response::<ErrorStatus>(
                        NockAppGrpcError::InvalidRequest("Address is improperly formatted".into()),
                    );
                    return timed_return(
                        &metrics.balance_update_error,
                        request_start,
                        Ok(Response::new(WalletGetBalanceResponse {
                            result: Some(wallet_get_balance_response::Result::Error(err)),
                        })),
                    );
                };

                if let Some(ref cur) = cursor {
                    if cur.key.address != address.key {
                        self.metrics
                            .balance_request_error_invalid_request_token_mismatch
                            .increment();
                        let err = self.build_error_response::<ErrorStatus>(
                            NockAppGrpcError::InvalidRequest(
                                "Page token does not match requested address".into(),
                            ),
                        );
                        return timed_return(
                            &metrics.balance_update_error,
                            request_start,
                            Ok(Response::new(WalletGetBalanceResponse {
                                result: Some(wallet_get_balance_response::Result::Error(err)),
                            })),
                        );
                    }
                }

                let mut cached: Option<Arc<CachedBalanceEntryAddress>> = None;

                if let Some(ref cursor) = cursor {
                    cached = self.cache_by_address.get(cursor.key())
                } else {
                    match self.cached_heaviest_chain().await {
                        Some((height, block_id)) => {
                            let cache_key = PageKeyAddress::new(
                                address.key.clone(),
                                height.0 .0,
                                block_id.clone(),
                            );
                            cached = self.cache_by_address.get(&cache_key);
                        }
                        None => {
                            warn!("Cache missed for heaviest chain, this should never happen except with a fresh nockchain node.");
                            self.metrics.heaviest_chain_cache_miss.increment();
                        }
                    }
                }

                if let Some(cached) = cached {
                    tracing::debug!("Cache hit for address: {}", address.key);
                    self.metrics.balance_cache_address_hit.increment();
                    match cached.build_paginated_response_address(
                        cursor.clone(),
                        client_page_items_limit,
                        max_bytes,
                        &self.metrics,
                    ) {
                        Ok(response) => {
                            return timed_return(
                                &metrics.balance_update_success_hit,
                                request_start,
                                Ok(Response::new(response)),
                            )
                        }
                        Err(err) => {
                            return timed_return(
                                &metrics.balance_update_error,
                                request_start,
                                Ok(Response::new(WalletGetBalanceResponse {
                                    result: Some(wallet_get_balance_response::Result::Error(err)),
                                })),
                            );
                        }
                    }
                }

                self.metrics.balance_cache_address_miss.increment();
                let path = vec!["balance-by-pubkey".to_string(), address.key.clone()];
                let mut path_slab = NounSlab::new();
                let path_noun = path.to_noun(&mut path_slab);
                path_slab.set_root(path_noun);

                let peek_start = Instant::now();
                let peek_result = self.handle.peek(path_slab).await;
                self.metrics
                    .balance_update_peek_time
                    .add_timing(&peek_start.elapsed());
                match peek_result {
                    Ok(Some(result_slab)) => {
                        let result_noun = unsafe { result_slab.root() };
                        let result = <Option<Option<v0::BalanceUpdate>>>::from_noun(&result_noun);

                        match result {
                            Ok(update) => {
                                let update = match update {
                                    // Peek result is double wrapped unit over the balance update
                                    Some(Some(update)) => update,
                                    Some(None) | None => {
                                        self.metrics.balance_request_error_peek_failed.increment();
                                        let err = self.build_error_response::<ErrorStatus>(
                                            NockAppGrpcError::PeekFailed,
                                        );
                                        return timed_return(
                                            &metrics.balance_update_error,
                                            request_start,
                                            Ok(Response::new(WalletGetBalanceResponse {
                                                result: Some(
                                                    wallet_get_balance_response::Result::Error(err),
                                                ),
                                            })),
                                        );
                                    }
                                };
                                let entry = self.cache_by_address.insert(&address.key, update);

                                match entry.build_paginated_response_address(
                                    cursor.clone(),
                                    client_page_items_limit,
                                    max_bytes,
                                    &self.metrics,
                                ) {
                                    Ok(response) => {
                                        return timed_return(
                                            &metrics.balance_update_success_miss,
                                            request_start,
                                            Ok(Response::new(response)),
                                        );
                                    }
                                    Err(err) => {
                                        return timed_return(
                                            &metrics.balance_update_error,
                                            request_start,
                                            Ok(Response::new(WalletGetBalanceResponse {
                                                result: Some(
                                                    wallet_get_balance_response::Result::Error(err),
                                                ),
                                            })),
                                        );
                                    }
                                }
                            }
                            Err(e) => {
                                self.metrics.balance_request_error_decode.increment();
                                let err = self.build_error_response::<ErrorStatus>(
                                    NockAppGrpcError::NounDecode(e),
                                );
                                return timed_return(
                                    &metrics.balance_update_error,
                                    request_start,
                                    Ok(Response::new(WalletGetBalanceResponse {
                                        result: Some(wallet_get_balance_response::Result::Error(
                                            err,
                                        )),
                                    })),
                                );
                            }
                        }
                    }
                    Ok(None) => {
                        self.metrics.balance_request_error_peek_failed.increment();
                        let err =
                            self.build_error_response::<ErrorStatus>(NockAppGrpcError::PeekFailed);
                        timed_return(
                            &metrics.balance_update_error,
                            request_start,
                            Ok(Response::new(WalletGetBalanceResponse {
                                result: Some(wallet_get_balance_response::Result::Error(err)),
                            })),
                        )
                    }
                    Err(e) => {
                        self.metrics.balance_request_error_nockapp.increment();
                        let err =
                            self.build_error_response::<ErrorStatus>(NockAppGrpcError::NockApp(e));
                        timed_return(
                            &metrics.balance_update_error,
                            request_start,
                            Ok(Response::new(WalletGetBalanceResponse {
                                result: Some(wallet_get_balance_response::Result::Error(err)),
                            })),
                        )
                    }
                }
            }
            Selector::FirstName(first_name_str) => {
                let cursor: Option<PageCursorFirstName> = if !token.is_empty() {
                    match decode_cursor_first_name(&token) {
                        Some(cur) => Some(cur),
                        None => {
                            self.metrics
                                .balance_request_error_invalid_request_token_invalid
                                .increment();
                            let err = ErrorStatus {
                                code: ErrorCode::InvalidRequest as i32,
                                message: "Invalid page token".into(),
                                details: None,
                            };
                            return timed_return(
                                &metrics.balance_update_error,
                                request_start,
                                Ok(Response::new(WalletGetBalanceResponse {
                                    result: Some(wallet_get_balance_response::Result::Error(err)),
                                })),
                            );
                        }
                    }
                } else {
                    None
                };

                let first_name: v1::Hash = match v1::Hash::from_base58(first_name_str.hash.as_str())
                {
                    Ok(hash) => hash,
                    Err(_) => {
                        self.metrics
                            .balance_request_error_invalid_request_invalid_first_name
                            .increment();
                        let err = self.build_error_response::<ErrorStatus>(
                            NockAppGrpcError::InvalidRequest("invalid first name hash".into()),
                        );
                        return timed_return(
                            &metrics.send_tx_error,
                            request_start,
                            Ok(Response::new(WalletGetBalanceResponse {
                                result: Some(wallet_get_balance_response::Result::Error(err)),
                            })),
                        );
                    }
                };

                if let Some(ref cur) = cursor {
                    if cur.key.first_name != first_name {
                        self.metrics
                            .balance_request_error_invalid_request_token_mismatch
                            .increment();
                        let err = self.build_error_response::<ErrorStatus>(
                            NockAppGrpcError::InvalidRequest(
                                "Page token does not match requested first name".into(),
                            ),
                        );
                        return timed_return(
                            &metrics.balance_update_error,
                            request_start,
                            Ok(Response::new(WalletGetBalanceResponse {
                                result: Some(wallet_get_balance_response::Result::Error(err)),
                            })),
                        );
                    }
                }

                let mut cached: Option<Arc<CachedBalanceEntryFirstName>> = None;

                if let Some(ref cursor) = cursor {
                    cached = self.cache_by_first_name.get(cursor.key())
                } else {
                    match self.cached_heaviest_chain().await {
                        Some((height, block_id)) => {
                            let cache_key = PageKeyFirstName::new(
                                first_name.clone(),
                                height.0 .0,
                                block_id.clone(),
                            );
                            cached = self.cache_by_first_name.get(&cache_key);
                        }
                        None => {
                            warn!("Cache missed for heaviest chain, this should never happen except with a fresh nockchain node.");
                            self.metrics.heaviest_chain_cache_miss.increment();
                        }
                    }
                }

                if let Some(cached) = cached {
                    tracing::debug!("Cache hit for first name: {:?}", first_name);
                    self.metrics.balance_cache_first_name_hit.increment();
                    match cached.build_paginated_response_first_name(
                        cursor.clone(),
                        client_page_items_limit,
                        max_bytes,
                        &self.metrics,
                    ) {
                        Ok(response) => {
                            return timed_return(
                                &metrics.balance_update_success_hit,
                                request_start,
                                Ok(Response::new(response)),
                            )
                        }
                        Err(err) => {
                            return timed_return(
                                &metrics.balance_update_error,
                                request_start,
                                Ok(Response::new(WalletGetBalanceResponse {
                                    result: Some(wallet_get_balance_response::Result::Error(err)),
                                })),
                            );
                        }
                    }
                }

                self.metrics.balance_cache_first_name_miss.increment();
                let path = vec!["balance-by-first-name".to_string(), first_name_str.hash];
                let mut path_slab = NounSlab::new();
                let path_noun = path.to_noun(&mut path_slab);
                path_slab.set_root(path_noun);

                let peek_start = Instant::now();
                let peek_result = self.handle.peek(path_slab).await;
                self.metrics
                    .balance_update_peek_time
                    .add_timing(&peek_start.elapsed());

                match peek_result {
                    Ok(Some(result_slab)) => {
                        let result_noun = unsafe { result_slab.root() };
                        let result = <Option<Option<v1::BalanceUpdate>>>::from_noun(&result_noun);

                        match result {
                            Ok(update) => {
                                let update = match update {
                                    // Peek result is double wrapped unit over the balance update
                                    Some(Some(update)) => update,
                                    Some(None) | None => {
                                        self.metrics.balance_request_error_peek_failed.increment();
                                        let err = self.build_error_response::<ErrorStatus>(
                                            NockAppGrpcError::PeekFailed,
                                        );
                                        return timed_return(
                                            &metrics.balance_update_error,
                                            request_start,
                                            Ok(Response::new(WalletGetBalanceResponse {
                                                result: Some(
                                                    wallet_get_balance_response::Result::Error(err),
                                                ),
                                            })),
                                        );
                                    }
                                };
                                let entry = self.cache_by_first_name.insert(&first_name, update);

                                match entry.build_paginated_response_first_name(
                                    cursor.clone(),
                                    client_page_items_limit,
                                    max_bytes,
                                    &self.metrics,
                                ) {
                                    Ok(response) => {
                                        return timed_return(
                                            &metrics.balance_update_success_miss,
                                            request_start,
                                            Ok(Response::new(response)),
                                        )
                                    }
                                    Err(err) => {
                                        return timed_return(
                                            &metrics.balance_update_error,
                                            request_start,
                                            Ok(Response::new(WalletGetBalanceResponse {
                                                result: Some(
                                                    wallet_get_balance_response::Result::Error(err),
                                                ),
                                            })),
                                        );
                                    }
                                }
                            }
                            Err(e) => {
                                self.metrics.balance_request_error_decode.increment();
                                let err = self.build_error_response::<ErrorStatus>(
                                    NockAppGrpcError::NounDecode(e),
                                );
                                return timed_return(
                                    &metrics.balance_update_error,
                                    request_start,
                                    Ok(Response::new(WalletGetBalanceResponse {
                                        result: Some(wallet_get_balance_response::Result::Error(
                                            err,
                                        )),
                                    })),
                                );
                            }
                        }
                    }
                    Ok(None) => {
                        self.metrics.balance_request_error_peek_failed.increment();
                        let err =
                            self.build_error_response::<ErrorStatus>(NockAppGrpcError::PeekFailed);
                        timed_return(
                            &metrics.balance_update_error,
                            request_start,
                            Ok(Response::new(WalletGetBalanceResponse {
                                result: Some(wallet_get_balance_response::Result::Error(err)),
                            })),
                        )
                    }
                    Err(e) => {
                        self.metrics.balance_request_error_nockapp.increment();
                        let err =
                            self.build_error_response::<ErrorStatus>(NockAppGrpcError::NockApp(e));
                        timed_return(
                            &metrics.balance_update_error,
                            request_start,
                            Ok(Response::new(WalletGetBalanceResponse {
                                result: Some(wallet_get_balance_response::Result::Error(err)),
                            })),
                        )
                    }
                }
            }
        }
    }

    async fn wallet_send_transaction(
        &self,
        request: Request<WalletSendTransactionRequest>,
    ) -> std::result::Result<Response<WalletSendTransactionResponse>, Status> {
        let req = request.into_inner();
        let request_start = Instant::now();
        let metrics = &self.metrics;
        debug!("WalletSendTransaction tx_id={:?}", req.tx_id);
        let tx_id_pb = match req.tx_id.clone() {
            Some(id) => id,
            None => {
                self.metrics
                    .send_tx_error_invalid_request_tx_id_missing
                    .increment();
                let err = self.build_error_response::<ErrorStatus>(
                    NockAppGrpcError::InvalidRequest("tx_id is required".into()),
                );
                return timed_return(
                    &metrics.send_tx_error,
                    request_start,
                    Ok(Response::new(WalletSendTransactionResponse {
                        result: Some(wallet_send_transaction_response::Result::Error(err)),
                    })),
                );
            }
        };

        let raw_tx_pb = match req.raw_tx.clone() {
            Some(raw) => raw,
            None => {
                self.metrics
                    .send_tx_error_invalid_request_raw_tx_missing
                    .increment();
                let err = self.build_error_response::<ErrorStatus>(
                    NockAppGrpcError::InvalidRequest("raw_tx is required".into()),
                );
                return timed_return(
                    &metrics.send_tx_error,
                    request_start,
                    Ok(Response::new(WalletSendTransactionResponse {
                        result: Some(wallet_send_transaction_response::Result::Error(err)),
                    })),
                );
            }
        };

        let tx_id_domain: v0::Hash = match tx_id_pb.clone().try_into() {
            Ok(id) => id,
            Err(_) => {
                self.metrics
                    .send_tx_error_invalid_request_tx_id_invalid
                    .increment();
                let err = self.build_error_response::<ErrorStatus>(
                    NockAppGrpcError::InvalidRequest("invalid tx_id".into()),
                );
                return timed_return(
                    &metrics.send_tx_error,
                    request_start,
                    Ok(Response::new(WalletSendTransactionResponse {
                        result: Some(wallet_send_transaction_response::Result::Error(err)),
                    })),
                );
            }
        };

        let raw_tx: v1::RawTx = match raw_tx_pb.clone().try_into() {
            Ok(tx) => tx,
            Err(e) => {
                self.metrics
                    .send_tx_error_invalid_request_raw_tx_invalid
                    .increment();
                let err = self.build_error_response::<ErrorStatus>(
                    NockAppGrpcError::InvalidRequest(format!("invalid raw_tx: {}", e)),
                );
                return timed_return(
                    &metrics.send_tx_error,
                    request_start,
                    Ok(Response::new(WalletSendTransactionResponse {
                        result: Some(wallet_send_transaction_response::Result::Error(err)),
                    })),
                );
            }
        };

        if raw_tx.id != tx_id_domain {
            self.metrics
                .send_tx_error_invalid_request_tx_id_mismatch
                .increment();
            let err = self.build_error_response::<ErrorStatus>(NockAppGrpcError::InvalidRequest(
                "tx_id does not match raw_tx.id".to_string(),
            ));
            return timed_return(
                &metrics.send_tx_error,
                request_start,
                Ok(Response::new(WalletSendTransactionResponse {
                    result: Some(wallet_send_transaction_response::Result::Error(err)),
                })),
            );
        }

        let mut payload_slab = NounSlab::new();
        let fact = nockapp::utils::make_tas(&mut payload_slab, "fact").as_noun();
        let heard_tx = nockapp::utils::make_tas(&mut payload_slab, "heard-tx").as_noun();
        let zero = nockvm::noun::D(0);
        let raw_noun = raw_tx.to_noun(&mut payload_slab);
        let heard_cell = nockvm::noun::T(&mut payload_slab, &[heard_tx, raw_noun]);
        let cause = nockvm::noun::T(&mut payload_slab, &[fact, zero, heard_cell]);
        payload_slab.set_root(cause);

        let wire = match grpc_wire_to_nockapp(&create_grpc_wire()) {
            Ok(w) => w,
            Err(e) => {
                let err = self.build_error_response::<ErrorStatus>(e);
                self.metrics.send_tx_error_internal.increment();
                return timed_return(
                    &metrics.send_tx_error,
                    request_start,
                    Ok(Response::new(WalletSendTransactionResponse {
                        result: Some(wallet_send_transaction_response::Result::Error(err)),
                    })),
                );
            }
        };

        let started_poke = Instant::now();
        let poke_result = self.handle.poke(wire, payload_slab).await;
        metrics
            .send_tx_poke_time
            .add_timing(&started_poke.elapsed());
        match poke_result {
            Ok(nockapp::driver::PokeResult::Ack) => timed_return(
                &metrics.send_tx_success,
                request_start,
                Ok(Response::new(WalletSendTransactionResponse {
                    result: Some(wallet_send_transaction_response::Result::Ack(
                        Acknowledged {},
                    )),
                })),
            ),
            Ok(nockapp::driver::PokeResult::Nack) => {
                self.metrics.send_tx_error_poke_failed.increment();
                let err = self.build_error_response::<ErrorStatus>(NockAppGrpcError::PokeFailed);
                timed_return(
                    &metrics.send_tx_error,
                    request_start,
                    Ok(Response::new(WalletSendTransactionResponse {
                        result: Some(wallet_send_transaction_response::Result::Error(err)),
                    })),
                )
            }
            Err(e) => {
                self.metrics.send_tx_error_nockapp.increment();
                let err = self.build_error_response::<ErrorStatus>(NockAppGrpcError::NockApp(e));
                timed_return(
                    &metrics.send_tx_error,
                    request_start,
                    Ok(Response::new(WalletSendTransactionResponse {
                        result: Some(wallet_send_transaction_response::Result::Error(err)),
                    })),
                )
            }
        }
    }

    async fn transaction_accepted(
        &self,
        request: Request<TransactionAcceptedRequest>,
    ) -> std::result::Result<Response<TransactionAcceptedResponse>, Status> {
        let req = request.into_inner();
        let request_start = Instant::now();
        let metrics = &self.metrics;
        debug!("TransactionAccepted tx_id={:?}", req.tx_id);

        let Some(pb_hash) = req.tx_id else {
            self.metrics
                .tx_accepted_error_invalid_request_missing_tx_id
                .increment();
            let err = self.build_error_response::<ErrorStatus>(NockAppGrpcError::InvalidRequest(
                "tx_id is required".into(),
            ));
            return timed_return(
                &metrics.tx_accepted_error,
                request_start,
                Ok(Response::new(TransactionAcceptedResponse {
                    result: Some(transaction_accepted_response::Result::Error(err)),
                })),
            );
        };

        let tx_id: String = pb_hash.hash.into();
        if tx_id.is_empty() {
            self.metrics
                .tx_accepted_error_invalid_request_empty_tx_id
                .increment();
            let err = self.build_error_response::<ErrorStatus>(NockAppGrpcError::InvalidRequest(
                "tx_id is required".into(),
            ));
            return timed_return(
                &metrics.tx_accepted_error,
                request_start,
                Ok(Response::new(TransactionAcceptedResponse {
                    result: Some(transaction_accepted_response::Result::Error(err)),
                })),
            );
        }

        let mut path_slab = NounSlab::new();
        let tag = nockapp::utils::make_tas(&mut path_slab, "tx-accepted").as_noun();
        let tx_id_noun: nockvm::noun::Noun = tx_id.to_noun(&mut path_slab);
        let path_noun = nockvm::noun::T(&mut path_slab, &[tag, tx_id_noun, SIG]);
        path_slab.set_root(path_noun);

        let start_peek = Instant::now();
        let peek_result = self.handle.peek(path_slab).await;
        metrics
            .tx_accepted_peek_time
            .add_timing(&start_peek.elapsed());
        match peek_result {
            Ok(Some(result_slab)) => {
                let result_noun = unsafe { result_slab.root() };
                match <Option<Option<bool>>>::from_noun(&result_noun) {
                    Ok(opt) => {
                        let accepted = opt.flatten().unwrap_or(false);
                        timed_return(
                            &metrics.tx_accepted_success,
                            request_start,
                            Ok(Response::new(TransactionAcceptedResponse {
                                result: Some(transaction_accepted_response::Result::Accepted(
                                    accepted,
                                )),
                            })),
                        )
                    }
                    Err(e) => {
                        self.metrics.tx_accepted_error_decode.increment();
                        let err = self
                            .build_error_response::<ErrorStatus>(NockAppGrpcError::NounDecode(e));
                        timed_return(
                            &metrics.tx_accepted_error,
                            request_start,
                            Ok(Response::new(TransactionAcceptedResponse {
                                result: Some(transaction_accepted_response::Result::Error(err)),
                            })),
                        )
                    }
                }
            }
            Ok(None) => {
                self.metrics.tx_accepted_error_peek_failed.increment();
                let err = self.build_error_response::<ErrorStatus>(NockAppGrpcError::PeekFailed);
                timed_return(
                    &metrics.tx_accepted_error,
                    request_start,
                    Ok(Response::new(TransactionAcceptedResponse {
                        result: Some(transaction_accepted_response::Result::Error(err)),
                    })),
                )
            }
            Err(e) => {
                self.metrics.tx_accepted_error_nockapp.increment();
                let err = self.build_error_response::<ErrorStatus>(NockAppGrpcError::NockApp(e));
                timed_return(
                    &metrics.tx_accepted_error,
                    request_start,
                    Ok(Response::new(TransactionAcceptedResponse {
                        result: Some(transaction_accepted_response::Result::Error(err)),
                    })),
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    use nockapp_grpc_proto::pb::common::v1::Base58Hash;
    use nockchain_math::crypto::cheetah::A_GEN;
    use nockchain_types::v1::Hash;

    use super::*;
    use crate::pb::common::{v1 as pb_common_v1, v2 as pb_common_v2};
    use crate::public_nockchain::v1::fixtures as fixtures_v1;
    use crate::public_nockchain::v2::fixtures;
    use crate::v2::pagination::cmp_name;

    struct MockHandleV0 {
        update: v0::BalanceUpdate,
        peek_calls: AtomicUsize,
    }

    impl MockHandleV0 {
        fn new(update: v0::BalanceUpdate) -> Self {
            Self {
                update,
                peek_calls: AtomicUsize::new(0),
            }
        }

        fn peek_calls(&self) -> usize {
            self.peek_calls.load(Ordering::SeqCst)
        }
    }

    struct MockHandle {
        update: v1::BalanceUpdate,
        peek_calls: AtomicUsize,
    }

    impl MockHandle {
        fn new(update: v1::BalanceUpdate) -> Self {
            Self {
                update,
                peek_calls: AtomicUsize::new(0),
            }
        }

        fn peek_calls(&self) -> usize {
            self.peek_calls.load(Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl BalanceHandle for MockHandle {
        async fn peek(
            &self,
            path: NounSlab,
        ) -> std::result::Result<Option<NounSlab>, nockapp::nockapp::error::NockAppError> {
            let root = unsafe { path.root() };
            if let Ok(segments) = <Vec<String>>::from_noun(&root) {
                if segments.first().map(String::as_str) == Some("heaviest-chain") {
                    let mut slab = NounSlab::new();
                    let noun = Some(Some((
                        self.update.height.clone(),
                        self.update.block_id.clone(),
                    )))
                    .to_noun(&mut slab);
                    slab.set_root(noun);
                    return Ok(Some(slab));
                }
            }

            let call = self.peek_calls.fetch_add(1, Ordering::SeqCst);
            assert_eq!(call, 0, "unexpected additional peek");
            Ok(Some(encode_balance_update(&self.update)))
        }

        async fn poke(
            &self,
            _wire: WireRepr,
            _payload: NounSlab,
        ) -> std::result::Result<PokeResult, nockapp::nockapp::error::NockAppError> {
            Err(nockapp::nockapp::error::NockAppError::OtherError(
                "poke not supported in mock".into(),
            ))
        }
    }

    #[async_trait]
    impl BalanceHandle for MockHandleV0 {
        async fn peek(
            &self,
            path: NounSlab,
        ) -> std::result::Result<Option<NounSlab>, nockapp::nockapp::error::NockAppError> {
            let root = unsafe { path.root() };
            if let Ok(segments) = <Vec<String>>::from_noun(&root) {
                if segments.first().map(String::as_str) == Some("heaviest-chain") {
                    let mut slab = NounSlab::new();
                    let noun = Some(Some((
                        self.update.height.clone(),
                        self.update.block_id.clone(),
                    )))
                    .to_noun(&mut slab);
                    slab.set_root(noun);
                    return Ok(Some(slab));
                }
            }

            let call = self.peek_calls.fetch_add(1, Ordering::SeqCst);
            assert_eq!(call, 0, "unexpected additional peek");
            Ok(Some(encode_balance_update_v0(&self.update)))
        }

        async fn poke(
            &self,
            _wire: WireRepr,
            _payload: NounSlab,
        ) -> std::result::Result<PokeResult, nockapp::nockapp::error::NockAppError> {
            Err(nockapp::nockapp::error::NockAppError::OtherError(
                "poke not supported in mock".into(),
            ))
        }
    }

    #[tokio::test]
    async fn wallet_get_balance_uses_cache_for_subsequent_pages() {
        let (update, expected_names) = fixtures_v1::make_balance_update(4);
        let handle = Arc::new(MockHandleV0::new(update));
        let server = PublicNockchainGrpcServer::with_handle(handle.clone());

        let mut request = WalletGetBalanceRequest {
            selector: Some(wallet_get_balance_request::Selector::Address(
                pb_common_v1::Base58Pubkey {
                    key: A_GEN.into_base58().expect("address generation failed"),
                },
            )),
            page: Some(pb_common_v1::PageRequest {
                client_page_items_limit: 2,
                page_token: String::new(),
                max_bytes: 0,
            }),
        };

        let first_resp = server
            .wallet_get_balance(Request::new(request.clone()))
            .await
            .expect("first call ok")
            .into_inner();

        let first_balance = match first_resp.result {
            Some(wallet_get_balance_response::Result::Balance(balance)) => balance,
            other => panic!("unexpected response: {:?}", other),
        };

        assert_eq!(first_balance.notes.len(), 2);
        let mut collected_names: Vec<pb_common_v1::Name> = first_balance
            .notes
            .iter()
            .map(|entry| entry.name.clone().expect("balance entry missing name"))
            .collect();

        let next_page_token = first_balance.page.expect("page info").next_page_token;
        assert!(!next_page_token.is_empty(), "expected non-empty page token");

        request.page = Some(pb_common_v1::PageRequest {
            client_page_items_limit: 2,
            page_token: next_page_token,
            max_bytes: 0,
        });

        let second_resp = server
            .wallet_get_balance(Request::new(request))
            .await
            .expect("second call ok")
            .into_inner();

        let second_balance = match second_resp.result {
            Some(wallet_get_balance_response::Result::Balance(balance)) => balance,
            other => panic!("unexpected response: {:?}", other),
        };

        collected_names.extend(second_balance.notes.into_iter().map(|entry| {
            entry
                .name
                .expect("balance entry missing name on second page")
        }));

        let mut collected_sorted: Vec<v1::Name> = collected_names
            .into_iter()
            .map(|name| name.try_into().expect("convert name"))
            .collect();
        collected_sorted.sort_by(cmp_name);

        let mut expected_sorted = expected_names.clone();
        expected_sorted.sort_by(cmp_name);

        assert_eq!(collected_sorted, expected_sorted);
        assert_eq!(handle.peek_calls(), 1, "cache should prevent second peek");
    }

    #[tokio::test]
    async fn wallet_get_balance_by_first_name_uses_cache_for_subsequent_pages() {
        // TODO: finish test
        let (update, expected_names) = fixtures::make_balance_update(4);
        let handle = Arc::new(MockHandle::new(update));
        let server = PublicNockchainGrpcServer::with_handle(handle.clone());

        let (name, _) = fixtures::make_named_note(15 as u64);

        let mut request = WalletGetBalanceRequest {
            selector: Some(wallet_get_balance_request::Selector::FirstName(
                Base58Hash {
                    hash: Hash::to_base58(&name.first),
                },
            )),
            page: Some(pb_common_v1::PageRequest {
                client_page_items_limit: 2,
                page_token: String::new(),
                max_bytes: 0,
            }),
        };

        let first_resp = server
            .wallet_get_balance(Request::new(request.clone()))
            .await
            .expect("first call ok")
            .into_inner();

        let first_balance = match first_resp.result {
            Some(wallet_get_balance_response::Result::Balance(balance)) => balance,
            other => panic!("unexpected response: {:?}", other),
        };

        assert_eq!(first_balance.notes.len(), 2);
        let mut collected_names: Vec<pb_common_v1::Name> = first_balance
            .notes
            .iter()
            .map(|entry| entry.name.clone().expect("balance entry missing name"))
            .collect();

        let next_page_token = first_balance.page.expect("page info").next_page_token;
        assert!(!next_page_token.is_empty(), "expected non-empty page token");

        request.page = Some(pb_common_v1::PageRequest {
            client_page_items_limit: 2,
            page_token: next_page_token,
            max_bytes: 0,
        });

        let second_resp = server
            .wallet_get_balance(Request::new(request))
            .await
            .expect("second call ok")
            .into_inner();

        let second_balance = match second_resp.result {
            Some(wallet_get_balance_response::Result::Balance(balance)) => balance,
            other => panic!("unexpected response: {:?}", other),
        };

        collected_names.extend(second_balance.notes.into_iter().map(|entry| {
            entry
                .name
                .expect("balance entry missing name on second page")
        }));

        let mut collected_sorted: Vec<v1::Name> = collected_names
            .into_iter()
            .map(|name| name.try_into().expect("convert name"))
            .collect();
        collected_sorted.sort_by(cmp_name);

        let mut expected_sorted = expected_names.clone();
        expected_sorted.sort_by(cmp_name);

        assert_eq!(collected_sorted, expected_sorted);
        assert_eq!(handle.peek_calls(), 1, "cache should prevent second peek");
    }

    fn encode_balance_update(update: &v1::BalanceUpdate) -> NounSlab {
        let mut slab = NounSlab::new();
        let noun = Some(Some(update.clone())).to_noun(&mut slab);
        slab.set_root(noun);
        slab
    }

    fn encode_balance_update_v0(update: &v0::BalanceUpdate) -> NounSlab {
        let mut slab = NounSlab::new();
        let noun = Some(Some(update.clone())).to_noun(&mut slab);
        slab.set_root(noun);
        slab
    }
}
