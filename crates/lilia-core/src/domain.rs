use lilia_contracts::{
    ConflictKind, ExpectedRevision, ProductError, ProductResult, ProductRevision,
    AGENT_TODO_PROMOTION_REQUIRED,
};

pub fn ensure_expected_revision(
    expected: ExpectedRevision,
    actual: ProductRevision,
) -> ProductResult<()> {
    if expected.matches(actual) {
        Ok(())
    } else {
        Err(ProductError::Conflict {
            conflict: ConflictKind::StaleRevision,
            message: format!(
                "expected revision {}, actual {}",
                expected.get(),
                actual.get()
            ),
        })
    }
}

/// Agent Todo titles become Product Tasks only through explicit promotion.
pub fn promote_agent_todo_title(title: &str) -> ProductResult<String> {
    let title = title.trim();
    if title.is_empty() {
        return Err(ProductError::InvalidInput {
            field: "title".into(),
            message: format!("{AGENT_TODO_PROMOTION_REQUIRED}: empty title"),
        });
    }
    Ok(title.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lilia_contracts::ProductRevision;

    #[test]
    fn stale_revision_is_conflict() {
        let expected = ExpectedRevision::new(1).unwrap();
        let actual = ProductRevision::new(2).unwrap();
        let err = ensure_expected_revision(expected, actual).unwrap_err();
        assert!(matches!(
            err,
            ProductError::Conflict {
                conflict: ConflictKind::StaleRevision,
                ..
            }
        ));
    }
}
