use crate::eth::{
    beacon::{BeaconError, BeaconResult, ToBlobSidecarsResponse},
    EthApi,
};
use alloy_rpc_types::BlockId;
use axum::{
    extract::{Path, State},
    response::{IntoResponse, Response},
};

/// Handles incoming Beacon API requests for blob sidecars
/// 
/// GET /eth/v1/beacon/blob_sidecars/{block_id}
pub async fn handle_get_blob_sidecars(
    State(api): State<EthApi>,
    Path(block_id): Path<String>,
) -> Response {
    match get_blob_sidecars_impl(api, block_id).await {
        Ok(response) => response.into_response(),
        Err(error) => error.into_response(),
    }
}

/// Implementation of the blob sidecars endpoint
async fn get_blob_sidecars_impl(
    api: EthApi,
    block_id: String,
) -> BeaconResult<impl IntoResponse> {
    // Parse block_id from path parameter
    let block_id = parse_block_id(&block_id)
        .map_err(|e| BeaconError::bad_request(format!("Invalid block_id: {}", e)))?;

    // Get the blob sidecars using existing EthApi logic
    let sidecar = api
        .anvil_get_blob_sidecars_by_block_id(block_id)
        .map_err(|e| BeaconError::internal_error(format!("Failed to get sidecars: {}", e)))?
        .ok_or_else(|| BeaconError::not_found("Block not found or no blobs in block"))?;

    // Convert to beacon API response
    Ok(sidecar.to_beacon_response())
}

/// Parse block_id from string path parameter
fn parse_block_id(block_id: &str) -> Result<BlockId, String> {
    // Handle special tags: "head", "finalized", "latest", "genesis"
    match block_id {
        "head" | "finalized" | "latest" => {
            // For Anvil, "head", "finalized", and "latest" all map to latest
            Ok(BlockId::Number(alloy_rpc_types::BlockNumberOrTag::Latest))
        }
        "genesis" => {
            Ok(BlockId::Number(alloy_rpc_types::BlockNumberOrTag::Earliest))
        }
        _ => {
            // Try to parse as number (decimal)
            if let Ok(num) = block_id.parse::<u64>() {
                return Ok(BlockId::Number(alloy_rpc_types::BlockNumberOrTag::Number(num)));
            }
            
            // Try to parse as hex
            if let Some(hex_str) = block_id.strip_prefix("0x") {
                if let Ok(num) = u64::from_str_radix(hex_str, 16) {
                    return Ok(BlockId::Number(alloy_rpc_types::BlockNumberOrTag::Number(num)));
                }
                // Try to parse as block hash
                if let Ok(hash) = hex_str.parse::<alloy_primitives::B256>() {
                    return Ok(BlockId::Hash(hash.into()));
                }
            }
            
            Err(format!("Unable to parse block_id: {}", block_id))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_rpc_types::BlockNumberOrTag;

    #[test]
    fn test_parse_block_id_head() {
        let result = parse_block_id("head").unwrap();
        assert_eq!(result, BlockId::Number(BlockNumberOrTag::Latest));
    }

    #[test]
    fn test_parse_block_id_finalized() {
        let result = parse_block_id("finalized").unwrap();
        assert_eq!(result, BlockId::Number(BlockNumberOrTag::Latest));
    }

    #[test]
    fn test_parse_block_id_genesis() {
        let result = parse_block_id("genesis").unwrap();
        assert_eq!(result, BlockId::Number(BlockNumberOrTag::Earliest));
    }

    #[test]
    fn test_parse_block_id_number() {
        let result = parse_block_id("12345").unwrap();
        assert_eq!(result, BlockId::Number(BlockNumberOrTag::Number(12345)));
    }

    #[test]
    fn test_parse_block_id_hex() {
        let result = parse_block_id("0x10").unwrap();
        assert_eq!(result, BlockId::Number(BlockNumberOrTag::Number(16)));
    }
}

