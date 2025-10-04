//! Blob sidecar specific types for Beacon API

use super::BeaconResponse;
use alloy_eips::eip4844::BlobTransactionSidecar;
use alloy_primitives::B256;
use alloy_rpc_types_beacon::{
    header::Header,
    sidecar::BlobData,
};

/// Beacon API response for blob sidecars
pub type BlobSidecarsResponse = BeaconResponse<Vec<BlobData>>;

/// Extension trait for converting `BlobTransactionSidecar` to Beacon API response
pub trait ToBlobSidecarsResponse {
    /// Converts to a beacon API blob sidecars response
    fn to_beacon_response(self) -> BlobSidecarsResponse;
}

impl ToBlobSidecarsResponse for BlobTransactionSidecar {
    fn to_beacon_response(self) -> BlobSidecarsResponse {
        let blob_data: Vec<BlobData> = self
            .blobs
            .into_iter()
            .zip(self.commitments.iter())
            .zip(self.proofs.iter())
            .enumerate()
            .map(|(index, ((blob, commitment), proof))| {
                // Create a minimal beacon block header for Anvil
                // These fields are not meaningful in Anvil's context but required by the spec
                let signed_block_header = Header::default();
                
                // Create the kzg_commitment_inclusion_proof
                // In Anvil, we can provide a zero proof as this is not validated
                let kzg_commitment_inclusion_proof = vec![B256::ZERO; 17];

                BlobData {
                    index: index as u64,
                    blob: blob.into(),
                    kzg_commitment: *commitment,
                    kzg_proof: *proof,
                    signed_block_header,
                    kzg_commitment_inclusion_proof,
                }
            })
            .collect();

        BeaconResponse::new(blob_data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_consensus::Blob;
    use alloy_primitives::FixedBytes;

    #[test]
    fn test_sidecar_to_beacon_response() {
        let blob = Blob::default();
        let commitment = FixedBytes::default();
        let proof = FixedBytes::default();

        let mut sidecar = BlobTransactionSidecar::default();
        sidecar.blobs.push(blob);
        sidecar.commitments.push(commitment);
        sidecar.proofs.push(proof);

        let response = sidecar.to_beacon_response();

        assert_eq!(response.data.len(), 1);
        assert!(!response.execution_optimistic.unwrap());
        assert!(!response.finalized.unwrap());
        assert_eq!(response.data[0].index, 0);
    }
}

