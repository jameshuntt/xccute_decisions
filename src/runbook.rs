//! Decision runbooks for bundling guides, plans, material contracts, and connector intent.
//!
//! A runbook is still source-level decision material. It does not execute. It says
//! which guide is allowed to ask questions, which runtime plan it applies to,
//! which material contract must gate the work, and which connector operation is
//! allowed to start the external call. Runtime remains responsible for verifying
//! materials, running connector calls, and producing receipts.

use crate::guide::{DecisionGuideTemplate, DecisionGuideTemplateError};
use xccute_runtime::{
    RuntimeConnectorCall,
    RuntimeConnectorError,
    RuntimeConnectorIdentity,
    RuntimeDecisionGuide,
    RuntimeMaterialManifest,
    RuntimeObservationPlan,
    RuntimeOperation,
    RuntimeOperationPlan,
    RuntimePlanMaterialContract,
    StableDigest,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecisionRunbookMode {
    ObservationOnly,
    ConnectorExecution,
}

impl DecisionRunbookMode {
    pub fn stable_label(&self) -> &'static str {
        match self {
            Self::ObservationOnly => "observation_only",
            Self::ConnectorExecution => "connector_execution",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecisionRunbookTemplateError {
    Guide(DecisionGuideTemplateError),
    MissingConnectorForExecution,
    UnexpectedConnectorForObservationOnly,
    StartOperationNotInPlan { operation_logical_id: String },
    MaterialContractPlanMismatch {
        plan_digest: StableDigest,
        material_plan_digest: StableDigest,
    },
    MaterialContractManifestMismatch {
        manifest_digest: StableDigest,
        material_manifest_digest: StableDigest,
    },
    Connector(RuntimeConnectorError),
}

impl From<DecisionGuideTemplateError> for DecisionRunbookTemplateError {
    fn from(value: DecisionGuideTemplateError) -> Self {
        Self::Guide(value)
    }
}

impl From<RuntimeConnectorError> for DecisionRunbookTemplateError {
    fn from(value: RuntimeConnectorError) -> Self {
        Self::Connector(value)
    }
}

pub type DecisionRunbookTemplateResult<T> = Result<T, DecisionRunbookTemplateError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionConnectorSpec {
    pub connector_id: String,
    pub connector_kind: String,
    pub service: String,
    pub function: String,
    pub operation_logical_id: String,
    pub requested_reason: String,
}

impl DecisionConnectorSpec {
    pub fn new(
        connector_id: impl Into<String>,
        connector_kind: impl Into<String>,
        service: impl Into<String>,
        function: impl Into<String>,
        operation_logical_id: impl Into<String>,
        requested_reason: impl Into<String>,
    ) -> Self {
        Self {
            connector_id: connector_id.into(),
            connector_kind: connector_kind.into(),
            service: service.into(),
            function: function.into(),
            operation_logical_id: operation_logical_id.into(),
            requested_reason: requested_reason.into(),
        }
    }

    pub fn nodeplan(
        connector_id: impl Into<String>,
        function: impl Into<String>,
        operation_logical_id: impl Into<String>,
        requested_reason: impl Into<String>,
    ) -> Self {
        Self::new(
            connector_id,
            "nodeplan",
            "nodeplan-control",
            function,
            operation_logical_id,
            requested_reason,
        )
    }

    pub fn denv(
        connector_id: impl Into<String>,
        function: impl Into<String>,
        operation_logical_id: impl Into<String>,
        requested_reason: impl Into<String>,
    ) -> Self {
        Self::new(
            connector_id,
            "denv",
            "denv-control",
            function,
            operation_logical_id,
            requested_reason,
        )
    }

    pub fn identity(&self) -> RuntimeConnectorIdentity {
        RuntimeConnectorIdentity::new(self.connector_id.clone(), self.connector_kind.clone())
    }

    pub fn operation<'a>(
        &self,
        plan: &'a RuntimeOperationPlan,
    ) -> DecisionRunbookTemplateResult<&'a RuntimeOperation> {
        plan.operations
            .iter()
            .find(|operation| operation.logical_id == self.operation_logical_id)
            .ok_or_else(|| DecisionRunbookTemplateError::StartOperationNotInPlan {
                operation_logical_id: self.operation_logical_id.clone(),
            })
    }

    pub fn materialize_call(
        &self,
        plan: &RuntimeOperationPlan,
    ) -> DecisionRunbookTemplateResult<RuntimeConnectorCall> {
        let operation = self.operation(plan)?;
        Ok(RuntimeConnectorCall::for_operation(
            self.identity(),
            self.service.clone(),
            self.function.clone(),
            plan,
            operation,
            self.requested_reason.clone(),
        )?)
    }

    pub fn stable_material(&self) -> String {
        let mut material = String::new();
        material.push_str("xccute.decisions.connector.spec.v1\n");
        push_stable_field(&mut material, "connector_id", &self.connector_id);
        push_stable_field(&mut material, "connector_kind", &self.connector_kind);
        push_stable_field(&mut material, "service", &self.service);
        push_stable_field(&mut material, "function", &self.function);
        push_stable_field(&mut material, "operation_logical_id", &self.operation_logical_id);
        push_stable_field(&mut material, "requested_reason", &self.requested_reason);
        material
    }

    pub fn digest(&self) -> StableDigest {
        StableDigest::sha256(self.stable_material())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionRunbookContract {
    pub runbook_id: String,
    pub purpose: String,
    pub mode: DecisionRunbookMode,
    pub guide_digest: StableDigest,
    pub path_digest: StableDigest,
    pub plan_id: String,
    pub plan_digest: StableDigest,
    pub material_contract_digest: StableDigest,
    pub connector_spec_digest: Option<StableDigest>,
    pub connector_call_digest: Option<StableDigest>,
    pub source_digest: StableDigest,
}

impl DecisionRunbookContract {
    pub fn stable_material(&self) -> String {
        let mut material = String::new();
        material.push_str("xccute.decisions.runbook.contract.v1\n");
        push_stable_field(&mut material, "runbook_id", &self.runbook_id);
        push_stable_field(&mut material, "purpose", &self.purpose);
        push_stable_field(&mut material, "mode", self.mode.stable_label());
        push_stable_field(&mut material, "guide_digest", self.guide_digest.as_str());
        push_stable_field(&mut material, "path_digest", self.path_digest.as_str());
        push_stable_field(&mut material, "plan_id", &self.plan_id);
        push_stable_field(&mut material, "plan_digest", self.plan_digest.as_str());
        push_stable_field(
            &mut material,
            "material_contract_digest",
            self.material_contract_digest.as_str(),
        );
        push_stable_field(
            &mut material,
            "connector_spec_digest",
            self.connector_spec_digest
                .as_ref()
                .map(StableDigest::as_str)
                .unwrap_or(""),
        );
        push_stable_field(
            &mut material,
            "connector_call_digest",
            self.connector_call_digest
                .as_ref()
                .map(StableDigest::as_str)
                .unwrap_or(""),
        );
        push_stable_field(&mut material, "source_digest", self.source_digest.as_str());
        material
    }

    pub fn digest(&self) -> StableDigest {
        StableDigest::sha256(self.stable_material())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionRunbookInstance {
    pub contract: DecisionRunbookContract,
    pub runtime_guide: RuntimeDecisionGuide,
    pub observation_plan: RuntimeObservationPlan,
    pub connector_call: Option<RuntimeConnectorCall>,
}

impl DecisionRunbookInstance {
    pub fn digest(&self) -> StableDigest {
        let mut material = String::new();
        material.push_str("xccute.decisions.runbook.instance.v1\n");
        push_stable_field(&mut material, "contract_digest", self.contract.digest().as_str());
        push_stable_field(&mut material, "runtime_guide_digest", self.runtime_guide.digest().as_str());
        push_stable_field(&mut material, "observation_plan_digest", self.observation_plan.digest().as_str());
        let connector_call_digest = self
            .connector_call
            .as_ref()
            .map(|call| call.digest().to_string())
            .unwrap_or_default();
        push_stable_field(&mut material, "connector_call_digest", &connector_call_digest);
        StableDigest::sha256(material)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionRunbookTemplate {
    pub runbook_id: String,
    pub purpose: String,
    pub mode: DecisionRunbookMode,
    pub guide: DecisionGuideTemplate,
    pub connector: Option<DecisionConnectorSpec>,
}

impl DecisionRunbookTemplate {
    pub fn observation_only(
        runbook_id: impl Into<String>,
        purpose: impl Into<String>,
        guide: DecisionGuideTemplate,
    ) -> Self {
        Self {
            runbook_id: runbook_id.into(),
            purpose: purpose.into(),
            mode: DecisionRunbookMode::ObservationOnly,
            guide,
            connector: None,
        }
    }

    pub fn connector_execution(
        runbook_id: impl Into<String>,
        purpose: impl Into<String>,
        guide: DecisionGuideTemplate,
        connector: DecisionConnectorSpec,
    ) -> Self {
        Self {
            runbook_id: runbook_id.into(),
            purpose: purpose.into(),
            mode: DecisionRunbookMode::ConnectorExecution,
            guide,
            connector: Some(connector),
        }
    }

    pub fn validate_connector_shape(&self) -> DecisionRunbookTemplateResult<()> {
        match (&self.mode, &self.connector) {
            (DecisionRunbookMode::ObservationOnly, None) => Ok(()),
            (DecisionRunbookMode::ObservationOnly, Some(_)) => {
                Err(DecisionRunbookTemplateError::UnexpectedConnectorForObservationOnly)
            }
            (DecisionRunbookMode::ConnectorExecution, Some(_)) => Ok(()),
            (DecisionRunbookMode::ConnectorExecution, None) => {
                Err(DecisionRunbookTemplateError::MissingConnectorForExecution)
            }
        }
    }

    pub fn stable_material(&self) -> String {
        let mut material = String::new();
        material.push_str("xccute.decisions.runbook.template.v1\n");
        push_stable_field(&mut material, "runbook_id", &self.runbook_id);
        push_stable_field(&mut material, "purpose", &self.purpose);
        push_stable_field(&mut material, "mode", self.mode.stable_label());
        push_stable_field(&mut material, "guide_digest", self.guide.digest().as_str());
        if let Some(connector) = &self.connector {
            push_stable_field(&mut material, "connector_digest", connector.digest().as_str());
            material.push_str(&connector.stable_material());
        } else {
            push_stable_field(&mut material, "connector_digest", "");
        }
        material.push_str(&self.guide.stable_material());
        material
    }

    pub fn digest(&self) -> StableDigest {
        StableDigest::sha256(self.stable_material())
    }

    pub fn materialize(
        &self,
        plan: &RuntimeOperationPlan,
        manifest: &RuntimeMaterialManifest,
        material_contract: &RuntimePlanMaterialContract,
        observation_plan_id: impl Into<String>,
    ) -> DecisionRunbookTemplateResult<DecisionRunbookInstance> {
        self.validate_connector_shape()?;
        self.guide.validate_path_coverage()?;

        let plan_digest = plan.digest();
        if material_contract.plan_digest != plan_digest {
            return Err(DecisionRunbookTemplateError::MaterialContractPlanMismatch {
                plan_digest,
                material_plan_digest: material_contract.plan_digest.clone(),
            });
        }

        let manifest_digest = manifest.digest();
        if material_contract.manifest_digest != manifest_digest {
            return Err(DecisionRunbookTemplateError::MaterialContractManifestMismatch {
                manifest_digest,
                material_manifest_digest: material_contract.manifest_digest.clone(),
            });
        }

        let runtime_guide = self.guide.materialize_runtime_guide(plan)?;
        let observation_plan = self
            .guide
            .materialize_observation_plan(plan, observation_plan_id)?;
        let connector_call = match &self.connector {
            Some(connector) => Some(connector.materialize_call(plan)?),
            None => None,
        };

        let contract = DecisionRunbookContract {
            runbook_id: self.runbook_id.clone(),
            purpose: self.purpose.clone(),
            mode: self.mode.clone(),
            guide_digest: self.guide.digest(),
            path_digest: self.guide.path.digest(),
            plan_id: plan.plan_id.clone(),
            plan_digest: plan.digest(),
            material_contract_digest: material_contract.digest(),
            connector_spec_digest: self.connector.as_ref().map(DecisionConnectorSpec::digest),
            connector_call_digest: connector_call.as_ref().map(RuntimeConnectorCall::digest),
            source_digest: self.digest(),
        };

        Ok(DecisionRunbookInstance {
            contract,
            runtime_guide,
            observation_plan,
            connector_call,
        })
    }
}

fn push_stable_field(out: &mut String, key: &str, value: &str) {
    out.push_str(key);
    out.push_str(".len=");
    out.push_str(&value.len().to_string());
    out.push('\n');
    out.push_str(key);
    out.push('=');
    out.push_str(value);
    out.push('\n');
}
