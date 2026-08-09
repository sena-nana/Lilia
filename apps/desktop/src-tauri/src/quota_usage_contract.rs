use std::sync::OnceLock;

use serde::Deserialize;

const QUOTA_CONTRACT_JSON: &str =
    include_str!("../../../../packages/contracts/src/quota-contract.json");

static QUOTA_CONTRACT: OnceLock<QuotaContract> = OnceLock::new();

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct QuotaContract {
    #[cfg(test)]
    commands: QuotaCommandsContract,
    usage_stats_days: Vec<i64>,
    default_usage_stats_days: i64,
    usage_stats_backend_filters: Vec<String>,
}

#[cfg(test)]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct QuotaCommandsContract {
    get_stats: String,
}

fn quota_contract() -> &'static QuotaContract {
    QUOTA_CONTRACT.get_or_init(|| {
        crate::contract_manifest::parse_contract_json(QUOTA_CONTRACT_JSON, "quota-contract.json")
    })
}

pub(crate) fn usage_stats_days() -> &'static [i64] {
    &quota_contract().usage_stats_days
}

pub(crate) fn default_usage_stats_days() -> i64 {
    quota_contract().default_usage_stats_days
}

pub(crate) fn usage_stats_backend_filters() -> &'static [String] {
    &quota_contract().usage_stats_backend_filters
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quota_usage::quota_usage_get_stats;

    #[test]
    fn quota_command_names_load_from_contract_manifest() {
        let commands = &quota_contract().commands;
        let _ = quota_usage_get_stats;
        assert_eq!(commands.get_stats, stringify!(quota_usage_get_stats));
    }
}
