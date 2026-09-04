//! Decision runbook records that bind source runbooks to runtime receipts.
//!
//! `DecisionRunbookTemplate` describes what should be asked and which verified
//! runtime contract it applies to. This module records the completed path: the
//! materialized runbook instance, compact guided evidence, acknowledged decision
//! path, optional connector execution receipt, optional observation receipt, and
//! optional append-only trace. It does not execute anything; it verifies that the
//! receipts being attached actually belong to the runbook instance.

use crate::runbook::{DecisionRunbookInstance, DecisionRunbookMode};
use xccute_runtime::{
    RuntimeAcknowledgedDecisionPath,
    RuntimeConnectorExecutionReceipt,
    RuntimeExecutionTrace,
    RuntimeGuidedDecisionContext,
    RuntimeObservationReceipt,
    StableDigest,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecisionRunbookRecordError {
    ExpectedObservationOnlyRunbook { actual: DecisionRunbookMode },
    ExpectedConnectorExecutionRunbook { actual: DecisionRunbookMode },
    ObservationOnlyRunbookHasConnectorCall,
    ConnectorExecutionRunbookMissingConnectorCall,
    GuideContextMismatch {
        expected_guide_digest: StableDigest,
        actual_guide_digest: StableDigest,
    },
    ObservationPlanContextMismatch {
        expected_observation_plan_digest: StableDigest,
        actual_observation_plan_digest: StableDigest,
    },
    AcknowledgedContextMismatch {
        expected_context_digest: StableDigest,
        actual_context_digest: StableDigest,
    },
    ConnectorCallMismatch {
        expected_call_digest: StableDigest,
        actual_call_digest: StableDigest,
    },
    AcknowledgedTransitionMismatch {
        expected_transition_digest: StableDigest,
        actual_transition_digest: StableDigest,
    },
    ObservationReceiptPlanMismatch {
        expected_observation_plan_digest: StableDigest,
        actual_observation_plan_digest: StableDigest,
    },
    ObservationReceiptExecutionMismatch {
        expected_execution_receipt_digest: StableDigest,
        actual_execution_receipt_digest: StableDigest,
    },
    TraceIsEmpty,
    TraceExecutionMismatch {
        expected_execution_receipt_digest: StableDigest,
        actual_execution_receipt_digest: StableDigest,
    },
    TraceObservationMismatch {
        expected_observation_receipt_digest: Option<StableDigest>,
        actual_observation_receipt_digest: Option<StableDigest>,
    },
}

pub type DecisionRunbookRecordResult<T> = Result<T, DecisionRunbookRecordError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionRunbookRecord {
    pub runbook_id: String,
    pub mode: DecisionRunbookMode,
    pub instance_digest: StableDigest,
    pub contract_digest: StableDigest,
    pub runtime_guide_digest: StableDigest,
    pub observation_plan_digest: StableDigest,
    pub guided_context_digest: StableDigest,
    pub acknowledged_path_digest: StableDigest,
    pub execution_receipt_digest: Option<StableDigest>,
    pub observation_receipt_digest: Option<StableDigest>,
    pub execution_trace_digest: Option<StableDigest>,
    pub recorded_reason: String,
}

impl DecisionRunbookRecord {
    pub fn observation_only(
        instance: &DecisionRunbookInstance,
        context: &RuntimeGuidedDecisionContext,
        acknowledged_path: &RuntimeAcknowledgedDecisionPath,
        recorded_reason: impl Into<String>,
    ) -> DecisionRunbookRecordResult<Self> {
        if instance.contract.mode != DecisionRunbookMode::ObservationOnly {
            return Err(DecisionRunbookRecordError::ExpectedObservationOnlyRunbook {
                actual: instance.contract.mode.clone(),
            });
        }

        if instance.connector_call.is_some() {
            return Err(DecisionRunbookRecordError::ObservationOnlyRunbookHasConnectorCall);
        }

        validate_context_links(instance, context, acknowledged_path)?;

        Ok(Self::from_parts(
            instance,
            context,
            acknowledged_path,
            None,
            None,
            None,
            recorded_reason,
        ))
    }

    pub fn connector_execution(
        instance: &DecisionRunbookInstance,
        context: &RuntimeGuidedDecisionContext,
        acknowledged_path: &RuntimeAcknowledgedDecisionPath,
        execution_receipt: &RuntimeConnectorExecutionReceipt,
        observation_receipt: Option<&RuntimeObservationReceipt>,
        trace: Option<&RuntimeExecutionTrace>,
        recorded_reason: impl Into<String>,
    ) -> DecisionRunbookRecordResult<Self> {
        if instance.contract.mode != DecisionRunbookMode::ConnectorExecution {
            return Err(DecisionRunbookRecordError::ExpectedConnectorExecutionRunbook {
                actual: instance.contract.mode.clone(),
            });
        }

        let expected_call_digest = instance
            .connector_call
            .as_ref()
            .ok_or(DecisionRunbookRecordError::ConnectorExecutionRunbookMissingConnectorCall)?
            .digest();
        let actual_call_digest = execution_receipt.connector_receipt.call.digest();
        if expected_call_digest != actual_call_digest {
            return Err(DecisionRunbookRecordError::ConnectorCallMismatch {
                expected_call_digest,
                actual_call_digest,
            });
        }

        validate_context_links(instance, context, acknowledged_path)?;

        let expected_acknowledged_transition = RuntimeAcknowledgedDecisionPath::new(
            context,
            &execution_receipt.connector_receipt.transition,
            acknowledged_path.acknowledged_reason.clone(),
        );
        if expected_acknowledged_transition.transition_digest != acknowledged_path.transition_digest {
            return Err(DecisionRunbookRecordError::AcknowledgedTransitionMismatch {
                expected_transition_digest: expected_acknowledged_transition.transition_digest,
                actual_transition_digest: acknowledged_path.transition_digest.clone(),
            });
        }

        if let Some(receipt) = observation_receipt {
            let expected_observation_plan_digest = instance.observation_plan.digest();
            if receipt.observation_plan_digest != expected_observation_plan_digest {
                return Err(DecisionRunbookRecordError::ObservationReceiptPlanMismatch {
                    expected_observation_plan_digest,
                    actual_observation_plan_digest: receipt.observation_plan_digest.clone(),
                });
            }

            let expected_execution_receipt_digest = execution_receipt.digest();
            if receipt.execution_receipt_digest != expected_execution_receipt_digest {
                return Err(DecisionRunbookRecordError::ObservationReceiptExecutionMismatch {
                    expected_execution_receipt_digest,
                    actual_execution_receipt_digest: receipt.execution_receipt_digest.clone(),
                });
            }
        }

        if let Some(trace) = trace {
            let last = trace.entries.last().ok_or(DecisionRunbookRecordError::TraceIsEmpty)?;
            let expected_execution_receipt_digest = execution_receipt.digest();
            if last.execution_receipt_digest != expected_execution_receipt_digest {
                return Err(DecisionRunbookRecordError::TraceExecutionMismatch {
                    expected_execution_receipt_digest,
                    actual_execution_receipt_digest: last.execution_receipt_digest.clone(),
                });
            }

            let expected_observation_receipt_digest = observation_receipt.map(RuntimeObservationReceipt::digest);
            if last.observation_receipt_digest != expected_observation_receipt_digest {
                return Err(DecisionRunbookRecordError::TraceObservationMismatch {
                    expected_observation_receipt_digest,
                    actual_observation_receipt_digest: last.observation_receipt_digest.clone(),
                });
            }
        }

        Ok(Self::from_parts(
            instance,
            context,
            acknowledged_path,
            Some(execution_receipt),
            observation_receipt,
            trace,
            recorded_reason,
        ))
    }

    fn from_parts(
        instance: &DecisionRunbookInstance,
        context: &RuntimeGuidedDecisionContext,
        acknowledged_path: &RuntimeAcknowledgedDecisionPath,
        execution_receipt: Option<&RuntimeConnectorExecutionReceipt>,
        observation_receipt: Option<&RuntimeObservationReceipt>,
        trace: Option<&RuntimeExecutionTrace>,
        recorded_reason: impl Into<String>,
    ) -> Self {
        Self {
            runbook_id: instance.contract.runbook_id.clone(),
            mode: instance.contract.mode.clone(),
            instance_digest: instance.digest(),
            contract_digest: instance.contract.digest(),
            runtime_guide_digest: instance.runtime_guide.digest(),
            observation_plan_digest: instance.observation_plan.digest(),
            guided_context_digest: context.digest(),
            acknowledged_path_digest: acknowledged_path.digest(),
            execution_receipt_digest: execution_receipt.map(RuntimeConnectorExecutionReceipt::digest),
            observation_receipt_digest: observation_receipt.map(RuntimeObservationReceipt::digest),
            execution_trace_digest: trace.map(RuntimeExecutionTrace::digest),
            recorded_reason: recorded_reason.into(),
        }
    }

    pub fn stable_material(&self) -> String {
        let mut material = String::new();
        material.push_str("xccute.decisions.runbook.record.v1\n");
        push_stable_field(&mut material, "runbook_id", &self.runbook_id);
        push_stable_field(&mut material, "mode", self.mode.stable_label());
        push_stable_field(&mut material, "instance_digest", self.instance_digest.as_str());
        push_stable_field(&mut material, "contract_digest", self.contract_digest.as_str());
        push_stable_field(&mut material, "runtime_guide_digest", self.runtime_guide_digest.as_str());
        push_stable_field(&mut material, "observation_plan_digest", self.observation_plan_digest.as_str());
        push_stable_field(&mut material, "guided_context_digest", self.guided_context_digest.as_str());
        push_stable_field(&mut material, "acknowledged_path_digest", self.acknowledged_path_digest.as_str());
        push_stable_field(
            &mut material,
            "execution_receipt_digest",
            self.execution_receipt_digest
                .as_ref()
                .map(StableDigest::as_str)
                .unwrap_or(""),
        );
        push_stable_field(
            &mut material,
            "observation_receipt_digest",
            self.observation_receipt_digest
                .as_ref()
                .map(StableDigest::as_str)
                .unwrap_or(""),
        );
        push_stable_field(
            &mut material,
            "execution_trace_digest",
            self.execution_trace_digest
                .as_ref()
                .map(StableDigest::as_str)
                .unwrap_or(""),
        );
        push_stable_field(&mut material, "recorded_reason", &self.recorded_reason);
        material
    }

    pub fn digest(&self) -> StableDigest {
        StableDigest::sha256(self.stable_material())
    }
}

fn validate_context_links(
    instance: &DecisionRunbookInstance,
    context: &RuntimeGuidedDecisionContext,
    acknowledged_path: &RuntimeAcknowledgedDecisionPath,
) -> DecisionRunbookRecordResult<()> {
    let expected_guide_digest = instance.runtime_guide.digest();
    if context.guide_digest != expected_guide_digest {
        return Err(DecisionRunbookRecordError::GuideContextMismatch {
            expected_guide_digest,
            actual_guide_digest: context.guide_digest.clone(),
        });
    }

    let expected_observation_plan_digest = instance.observation_plan.digest();
    if context.observation_plan_digest != expected_observation_plan_digest {
        return Err(DecisionRunbookRecordError::ObservationPlanContextMismatch {
            expected_observation_plan_digest,
            actual_observation_plan_digest: context.observation_plan_digest.clone(),
        });
    }

    let expected_context_digest = context.digest();
    if acknowledged_path.context_digest != expected_context_digest {
        return Err(DecisionRunbookRecordError::AcknowledgedContextMismatch {
            expected_context_digest,
            actual_context_digest: acknowledged_path.context_digest.clone(),
        });
    }

    Ok(())
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
