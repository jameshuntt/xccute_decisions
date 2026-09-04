//! Optional and required decision path templates.
//!
//! A decision path is the source-level plan for how a solution should be guided:
//! which observations are required, which checks are optional, which operations
//! can happen after evidence is available, and where a branch may occur. Runtime
//! receipts prove what actually happened; this module defines what was allowed to
//! be asked.

use xccute_runtime::StableDigest;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecisionPathStepKind {
    RequiredObservation,
    OptionalObservation,
    Operation,
    Branch,
    Stop,
}

impl DecisionPathStepKind {
    pub fn stable_label(&self) -> &'static str {
        match self {
            Self::RequiredObservation => "required_observation",
            Self::OptionalObservation => "optional_observation",
            Self::Operation => "operation",
            Self::Branch => "branch",
            Self::Stop => "stop",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionPathStep {
    pub step_id: String,
    pub kind: DecisionPathStepKind,
    pub tool_id: Option<String>,
    pub question_logical_id: Option<String>,
    pub operation_logical_id: Option<String>,
    pub reason: String,
}

impl DecisionPathStep {
    pub fn required_observation(
        step_id: impl Into<String>,
        tool_id: impl Into<String>,
        question_logical_id: impl Into<String>,
        operation_logical_id: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            step_id: step_id.into(),
            kind: DecisionPathStepKind::RequiredObservation,
            tool_id: Some(tool_id.into()),
            question_logical_id: Some(question_logical_id.into()),
            operation_logical_id: Some(operation_logical_id.into()),
            reason: reason.into(),
        }
    }

    pub fn optional_observation(
        step_id: impl Into<String>,
        tool_id: impl Into<String>,
        question_logical_id: impl Into<String>,
        operation_logical_id: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            step_id: step_id.into(),
            kind: DecisionPathStepKind::OptionalObservation,
            tool_id: Some(tool_id.into()),
            question_logical_id: Some(question_logical_id.into()),
            operation_logical_id: Some(operation_logical_id.into()),
            reason: reason.into(),
        }
    }

    pub fn operation(
        step_id: impl Into<String>,
        operation_logical_id: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            step_id: step_id.into(),
            kind: DecisionPathStepKind::Operation,
            tool_id: None,
            question_logical_id: None,
            operation_logical_id: Some(operation_logical_id.into()),
            reason: reason.into(),
        }
    }

    pub fn branch(step_id: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            step_id: step_id.into(),
            kind: DecisionPathStepKind::Branch,
            tool_id: None,
            question_logical_id: None,
            operation_logical_id: None,
            reason: reason.into(),
        }
    }

    pub fn stop(step_id: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            step_id: step_id.into(),
            kind: DecisionPathStepKind::Stop,
            tool_id: None,
            question_logical_id: None,
            operation_logical_id: None,
            reason: reason.into(),
        }
    }

    pub fn is_required_observation(&self) -> bool {
        self.kind == DecisionPathStepKind::RequiredObservation
    }

    pub fn stable_material(&self) -> String {
        let mut material = String::new();
        material.push_str("xccute.decisions.path.step.v1\n");
        push_stable_field(&mut material, "step_id", &self.step_id);
        push_stable_field(&mut material, "kind", self.kind.stable_label());
        push_stable_field(&mut material, "tool_id", self.tool_id.as_deref().unwrap_or(""));
        push_stable_field(
            &mut material,
            "question_logical_id",
            self.question_logical_id.as_deref().unwrap_or(""),
        );
        push_stable_field(
            &mut material,
            "operation_logical_id",
            self.operation_logical_id.as_deref().unwrap_or(""),
        );
        push_stable_field(&mut material, "reason", &self.reason);
        material
    }

    pub fn digest(&self) -> StableDigest {
        StableDigest::sha256(self.stable_material())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionPathTemplate {
    pub path_id: String,
    pub goal: String,
    pub steps: Vec<DecisionPathStep>,
}

impl DecisionPathTemplate {
    pub fn new(path_id: impl Into<String>, goal: impl Into<String>) -> Self {
        Self {
            path_id: path_id.into(),
            goal: goal.into(),
            steps: Vec::new(),
        }
    }

    pub fn step(mut self, step: DecisionPathStep) -> Self {
        self.steps.push(step);
        self
    }

    pub fn required_observation_count(&self) -> usize {
        self.steps
            .iter()
            .filter(|step| step.is_required_observation())
            .count()
    }

    pub fn includes_required_observation(&self, question_logical_id: &str) -> bool {
        self.steps.iter().any(|step| {
            step.is_required_observation()
                && step.question_logical_id.as_deref() == Some(question_logical_id)
        })
    }

    pub fn optional_steps(&self) -> impl Iterator<Item = &DecisionPathStep> {
        self.steps
            .iter()
            .filter(|step| step.kind == DecisionPathStepKind::OptionalObservation)
    }

    pub fn stable_material(&self) -> String {
        let mut material = String::new();
        material.push_str("xccute.decisions.path.template.v1\n");
        push_stable_field(&mut material, "path_id", &self.path_id);
        push_stable_field(&mut material, "goal", &self.goal);
        for (index, step) in self.steps.iter().enumerate() {
            material.push_str("step[");
            material.push_str(&index.to_string());
            material.push_str("].digest=");
            material.push_str(step.digest().as_str());
            material.push('\n');
            material.push_str(&step.stable_material());
        }
        material
    }

    pub fn digest(&self) -> StableDigest {
        StableDigest::sha256(self.stable_material())
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
