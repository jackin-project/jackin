// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::ptr;

use serde::{Deserialize, Serialize, Serializer};

use crate::display::plain;
use crate::{Agent, AgentTuple, CatalogError, ModelName, Provider};

fn is_proven_account_default_tuple(tuple: &'static AgentTuple) -> bool {
    crate::lookup_tuple(Agent::Codex, Provider::Codex).is_ok_and(|proven| ptr::eq(tuple, proven))
}

fn canonical_tuple(tuple: &'static AgentTuple) -> Result<&'static AgentTuple, ModelSelectionError> {
    let canonical = crate::lookup_tuple(tuple.agent, tuple.provider)?;
    if ptr::eq(tuple, canonical) {
        Ok(canonical)
    } else {
        Err(CatalogError::UnknownTuple {
            agent: tuple.agent,
            provider: tuple.provider,
        }
        .into())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ModelSelectionError {
    #[error("account default has no named model for agent {} with provider {}", plain(.agent), plain(.provider))]
    AccountDefaultNotNamed { agent: String, provider: String },
    #[error("account default is not supported for agent {} with provider {}", plain(.agent), plain(.provider))]
    AccountDefaultUnsupported { agent: String, provider: String },
    #[error(transparent)]
    UnknownModel(#[from] CatalogError),
    #[error("invalid default model in tuple {tuple}: {}", plain(.reason))]
    InvalidTupleDefault { tuple: &'static str, reason: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum SelectionKind {
    AccountDefault,
    Named(ModelName),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelSelection(SelectionKind);

#[derive(Serialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
enum SelectionRef<'a> {
    AccountDefault,
    Named { name: &'a ModelName },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum SelectionWire {
    AccountDefault {},
    Named { name: ModelName },
}

impl ModelSelection {
    pub fn for_tuple(
        tuple: &'static AgentTuple,
        explicit: Option<ModelName>,
    ) -> Result<ModelSelection, ModelSelectionError> {
        let tuple = canonical_tuple(tuple)?;
        match explicit {
            Some(model) => Self::named_for_tuple(tuple, model),
            None if is_proven_account_default_tuple(tuple) => {
                Ok(ModelSelection(SelectionKind::AccountDefault))
            }
            None => {
                let model = ModelName::parse(tuple.default_model).map_err(|source| {
                    ModelSelectionError::InvalidTupleDefault {
                        tuple: tuple.id,
                        reason: source.to_string(),
                    }
                })?;
                Self::named_for_tuple(tuple, model)
            }
        }
    }

    pub fn named_projection(
        &self,
        tuple: &'static AgentTuple,
    ) -> Result<&ModelName, ModelSelectionError> {
        let tuple = canonical_tuple(tuple)?;
        match &self.0 {
            SelectionKind::AccountDefault => Err(ModelSelectionError::AccountDefaultNotNamed {
                agent: tuple.agent.as_str().to_owned(),
                provider: tuple.provider.as_str().to_owned(),
            }),
            SelectionKind::Named(model) => {
                tuple.check_model(model.as_str())?;
                Ok(model)
            }
        }
    }

    pub fn validate_for_tuple(
        &self,
        tuple: &'static AgentTuple,
    ) -> Result<(), ModelSelectionError> {
        let tuple = canonical_tuple(tuple)?;
        match &self.0 {
            SelectionKind::AccountDefault if is_proven_account_default_tuple(tuple) => Ok(()),
            SelectionKind::AccountDefault => Err(ModelSelectionError::AccountDefaultUnsupported {
                agent: tuple.agent.as_str().to_owned(),
                provider: tuple.provider.as_str().to_owned(),
            }),
            SelectionKind::Named(model) => {
                tuple.check_model(model.as_str())?;
                Ok(())
            }
        }
    }

    pub fn account_default_for(
        tuple: &'static AgentTuple,
    ) -> Result<ModelSelection, ModelSelectionError> {
        let tuple = canonical_tuple(tuple)?;
        if is_proven_account_default_tuple(tuple) {
            Ok(ModelSelection(SelectionKind::AccountDefault))
        } else {
            Err(ModelSelectionError::AccountDefaultUnsupported {
                agent: tuple.agent.as_str().to_owned(),
                provider: tuple.provider.as_str().to_owned(),
            })
        }
    }

    pub fn named_for_tuple(
        tuple: &'static AgentTuple,
        model: ModelName,
    ) -> Result<ModelSelection, ModelSelectionError> {
        let tuple = canonical_tuple(tuple)?;
        tuple.check_model(model.as_str())?;
        Ok(ModelSelection(SelectionKind::Named(model)))
    }

    pub fn is_account_default(&self) -> bool {
        matches!(self.0, SelectionKind::AccountDefault)
    }

    pub fn named(&self, tuple: &'static AgentTuple) -> Result<&ModelName, ModelSelectionError> {
        self.named_projection(tuple)
    }

    pub fn item_key(&self) -> String {
        match &self.0 {
            SelectionKind::AccountDefault => "{\"mode\":\"account_default\"}".to_owned(),
            SelectionKind::Named(model) => {
                format!("{{\"mode\":\"named\",\"name\":\"{}\"}}", model.as_str())
            }
        }
    }

    pub fn from_wire(
        tuple: &'static AgentTuple,
        wire: SelectionWire,
    ) -> Result<ModelSelection, ModelSelectionError> {
        match wire {
            SelectionWire::AccountDefault {} => Self::account_default_for(tuple),
            SelectionWire::Named { name } => Self::named_for_tuple(tuple, name),
        }
    }
}

impl Serialize for ModelSelection {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match &self.0 {
            SelectionKind::AccountDefault => SelectionRef::AccountDefault.serialize(serializer),
            SelectionKind::Named(model) => {
                SelectionRef::Named { name: model }.serialize(serializer)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tuple(id: &str) -> &'static AgentTuple {
        crate::TUPLES
            .iter()
            .find(|tuple| tuple.id == id)
            .unwrap_or_else(|| panic!("tuple {id} is not in the catalog"))
    }

    #[test]
    fn forged_t01_id_does_not_grant_account_default() {
        static FORGED: AgentTuple = AgentTuple {
            id: "T01",
            agent: Agent::ClaudeCode,
            provider: Provider::Claude,
            auth_class: crate::AuthClass::OAuth,
            state: crate::TupleState::Supported,
            reason: None,
            allowed_models: &[],
            default_model: "",
        };
        assert!(matches!(
            ModelSelection::account_default_for(&FORGED),
            Err(ModelSelectionError::UnknownModel(
                CatalogError::UnknownTuple {
                    agent: Agent::ClaudeCode,
                    provider: Provider::Claude,
                }
            ))
        ));
    }

    #[test]
    fn t01_resolves_to_account_default() {
        let t01 = tuple("T01");
        let selection = ModelSelection::for_tuple(t01, None).expect("account default");
        assert!(selection.is_account_default());
        assert!(matches!(
            selection.named(t01),
            Err(ModelSelectionError::AccountDefaultNotNamed { .. })
        ));
        assert_eq!(selection.item_key(), "{\"mode\":\"account_default\"}");
    }

    #[test]
    fn t01_accepts_catalogued_named_models() {
        let t01 = tuple("T01");
        let model = ModelName::parse(t01.default_model).expect("default model");
        let selection = ModelSelection::named_for_tuple(t01, model).expect("catalogued model");
        assert_eq!(
            selection
                .named_projection(t01)
                .expect("projection")
                .as_str(),
            t01.default_model
        );
        assert_eq!(
            selection.item_key(),
            "{\"mode\":\"named\",\"name\":\"gpt-6.1-sol\"}"
        );
    }

    #[test]
    fn named_models_must_be_catalogued() {
        let t01 = tuple("T01");
        let t02 = tuple("T02");
        let model = ModelName::parse(t02.default_model).expect("model name");
        assert!(matches!(
            ModelSelection::named_for_tuple(t01, model),
            Err(ModelSelectionError::UnknownModel(_))
        ));
    }

    #[test]
    fn non_t01_rejects_account_default() {
        let other = crate::TUPLES
            .iter()
            .find(|tuple| tuple.id != "T01")
            .expect("non-T01 tuple");
        assert!(matches!(
            ModelSelection::account_default_for(other),
            Err(ModelSelectionError::AccountDefaultUnsupported { agent, provider })
                if agent == other.agent.as_str() && provider == other.provider.as_str()
        ));
    }

    #[test]
    fn non_t01_resolves_to_named_default() {
        let t02 = tuple("T02");
        let selection = ModelSelection::for_tuple(t02, None).expect("named default");
        let model = selection.named(t02).expect("named selection");
        assert_eq!(model.as_str(), t02.default_model);
        assert_eq!(selection.named_projection(t02).expect("projection"), model);
    }

    fn forged() -> &'static AgentTuple {
        static FORGED: AgentTuple = AgentTuple {
            id: "T01",
            agent: Agent::Codex,
            provider: Provider::Codex,
            auth_class: crate::AuthClass::OAuth,
            state: crate::TupleState::Supported,
            reason: None,
            allowed_models: &["gpt-6.1-sol", "evil"],
            default_model: "gpt-6.1-sol",
        };
        &FORGED
    }

    #[test]
    fn unproven_tuple_cannot_construct_named_selection() {
        let model = ModelName::parse("evil").expect("model name");
        assert!(matches!(
            ModelSelection::named_for_tuple(forged(), model),
            Err(ModelSelectionError::UnknownModel(
                CatalogError::UnknownTuple {
                    agent: Agent::Codex,
                    provider: Provider::Codex,
                }
            ))
        ));
    }

    #[test]
    fn unproven_tuple_cannot_construct_selection_with_explicit_model() {
        let model = ModelName::parse("gpt-6.1-sol").expect("model name");
        assert!(matches!(
            ModelSelection::for_tuple(forged(), Some(model)),
            Err(ModelSelectionError::UnknownModel(
                CatalogError::UnknownTuple {
                    agent: Agent::Codex,
                    provider: Provider::Codex,
                }
            ))
        ));
    }

    #[test]
    fn unproven_tuple_cannot_resolve_default_selection() {
        assert!(matches!(
            ModelSelection::for_tuple(forged(), None),
            Err(ModelSelectionError::UnknownModel(
                CatalogError::UnknownTuple {
                    agent: Agent::Codex,
                    provider: Provider::Codex,
                }
            ))
        ));
    }

    #[test]
    fn unproven_tuple_cannot_decode_named_wire() {
        let name = ModelName::parse("evil").expect("model name");
        assert!(matches!(
            ModelSelection::from_wire(forged(), SelectionWire::Named { name }),
            Err(ModelSelectionError::UnknownModel(
                CatalogError::UnknownTuple {
                    agent: Agent::Codex,
                    provider: Provider::Codex,
                }
            ))
        ));
    }

    #[test]
    fn unproven_tuple_cannot_validate_named_selection() {
        let t01 = tuple("T01");
        let model = ModelName::parse(t01.default_model).expect("model name");
        let selection = ModelSelection::named_for_tuple(t01, model).expect("named");
        assert!(matches!(
            selection.validate_for_tuple(forged()),
            Err(ModelSelectionError::UnknownModel(
                CatalogError::UnknownTuple {
                    agent: Agent::Codex,
                    provider: Provider::Codex,
                }
            ))
        ));
    }

    #[test]
    fn unproven_tuple_cannot_project_named_selection() {
        let t01 = tuple("T01");
        let model = ModelName::parse(t01.default_model).expect("model name");
        let selection = ModelSelection::named_for_tuple(t01, model).expect("named");
        assert!(matches!(
            selection.named_projection(forged()),
            Err(ModelSelectionError::UnknownModel(
                CatalogError::UnknownTuple {
                    agent: Agent::Codex,
                    provider: Provider::Codex,
                }
            ))
        ));
    }

    #[test]
    fn unproven_tuple_cannot_acquire_account_default() {
        assert!(matches!(
            ModelSelection::account_default_for(forged()),
            Err(ModelSelectionError::UnknownModel(
                CatalogError::UnknownTuple {
                    agent: Agent::Codex,
                    provider: Provider::Codex,
                }
            ))
        ));
    }

    #[test]
    fn unproven_tuple_cannot_decode_account_default_wire() {
        assert!(matches!(
            ModelSelection::from_wire(forged(), SelectionWire::AccountDefault {}),
            Err(ModelSelectionError::UnknownModel(
                CatalogError::UnknownTuple {
                    agent: Agent::Codex,
                    provider: Provider::Codex,
                }
            ))
        ));
    }

    #[test]
    fn unproven_tuple_cannot_validate_account_default() {
        let selection = ModelSelection::account_default_for(tuple("T01")).expect("account default");
        assert!(matches!(
            selection.validate_for_tuple(forged()),
            Err(ModelSelectionError::UnknownModel(
                CatalogError::UnknownTuple {
                    agent: Agent::Codex,
                    provider: Provider::Codex,
                }
            ))
        ));
    }

    #[test]
    fn unproven_tuple_cannot_project_account_default() {
        let selection = ModelSelection::account_default_for(tuple("T01")).expect("account default");
        assert!(matches!(
            selection.named_projection(forged()),
            Err(ModelSelectionError::UnknownModel(
                CatalogError::UnknownTuple {
                    agent: Agent::Codex,
                    provider: Provider::Codex,
                }
            ))
        ));
    }

    #[test]
    fn named_accessor_rejects_cross_tuple_pairing() {
        let t01 = tuple("T01");
        let t02 = tuple("T02");
        let model = ModelName::parse(t02.default_model).expect("model name");
        let selection = ModelSelection::named_for_tuple(t02, model).expect("named");
        assert!(matches!(
            selection.named(t01),
            Err(ModelSelectionError::UnknownModel(_))
        ));
    }

    #[test]
    fn account_default_projection_is_not_reported_unsupported() {
        let t01 = tuple("T01");
        let selection = ModelSelection::account_default_for(t01).expect("account default");
        let err = selection.named_projection(t01).expect_err("no named model");
        assert!(!matches!(
            err,
            ModelSelectionError::AccountDefaultUnsupported { .. }
        ));
        assert_eq!(
            err.to_string(),
            "account default has no named model for agent codex with provider codex"
        );
    }

    #[test]
    fn wire_round_trip_preserves_selection() {
        let t02 = tuple("T02");
        let model = ModelName::parse(t02.default_model).expect("default model");
        let selection = ModelSelection::named_for_tuple(t02, model).expect("named");
        let json = serde_json::to_string(&selection).expect("serialize");
        let wire: SelectionWire = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(
            ModelSelection::from_wire(t02, wire).expect("wire"),
            selection
        );
    }
}
