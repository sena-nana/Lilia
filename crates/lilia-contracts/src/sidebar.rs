use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const SIDEBAR_NAVIGATION_EXTENSION_ID: &str = "lilia.sidebar.navigation";
pub const SIDEBAR_NAVIGATION_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SidebarNavigationContributionSet {
    pub items: Vec<SidebarNavigationContribution>,
}

impl SidebarNavigationContributionSet {
    pub fn validate(&self) -> Result<(), SidebarNavigationContributionError> {
        let mut ids = BTreeSet::new();
        for item in &self.items {
            item.validate()?;
            if !ids.insert(item.id.as_str()) {
                return Err(SidebarNavigationContributionError::DuplicateId(
                    item.id.clone(),
                ));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SidebarNavigationContribution {
    pub id: String,
    pub label: String,
    pub icon: SidebarNavigationIcon,
    pub target: SidebarNavigationTarget,
    pub order: i32,
}

impl SidebarNavigationContribution {
    fn validate(&self) -> Result<(), SidebarNavigationContributionError> {
        if self.id.is_empty()
            || self.id.bytes().any(|byte| {
                !(byte.is_ascii_lowercase() || byte.is_ascii_digit() || b".-_".contains(&byte))
            })
        {
            return Err(SidebarNavigationContributionError::InvalidId(
                self.id.clone(),
            ));
        }
        if self.label.trim().is_empty() {
            return Err(SidebarNavigationContributionError::EmptyLabel(
                self.id.clone(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SidebarNavigationIcon {
    Settings,
    Automations,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SidebarNavigationTarget {
    Settings,
    Automations,
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum SidebarNavigationContributionError {
    #[error("sidebar navigation contribution has invalid id `{0}`")]
    InvalidId(String),
    #[error("sidebar navigation contribution `{0}` has an empty label")]
    EmptyLabel(String),
    #[error("sidebar navigation contribution contains duplicate id `{0}`")]
    DuplicateId(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contribution(id: &str, target: SidebarNavigationTarget) -> SidebarNavigationContribution {
        SidebarNavigationContribution {
            id: id.into(),
            label: id.into(),
            icon: match target {
                SidebarNavigationTarget::Settings => SidebarNavigationIcon::Settings,
                SidebarNavigationTarget::Automations => SidebarNavigationIcon::Automations,
            },
            target,
            order: 0,
        }
    }

    #[test]
    fn sidebar_navigation_contract_round_trips() {
        let expected = SidebarNavigationContributionSet {
            items: vec![contribution(
                "lilia.sidebar.footer.settings",
                SidebarNavigationTarget::Settings,
            )],
        };
        expected.validate().unwrap();

        let encoded = serde_json::to_value(&expected).unwrap();
        let decoded: SidebarNavigationContributionSet = serde_json::from_value(encoded).unwrap();

        assert_eq!(decoded, expected);
    }

    #[test]
    fn sidebar_navigation_contract_rejects_invalid_and_duplicate_ids() {
        let invalid = SidebarNavigationContributionSet {
            items: vec![contribution(
                "Lilia Sidebar",
                SidebarNavigationTarget::Settings,
            )],
        };
        assert!(matches!(
            invalid.validate(),
            Err(SidebarNavigationContributionError::InvalidId(_))
        ));

        let duplicate = SidebarNavigationContributionSet {
            items: vec![
                contribution("lilia.sidebar.item", SidebarNavigationTarget::Settings),
                contribution("lilia.sidebar.item", SidebarNavigationTarget::Automations),
            ],
        };
        assert!(matches!(
            duplicate.validate(),
            Err(SidebarNavigationContributionError::DuplicateId(_))
        ));
    }
}
