//! Compilation of a tree-shaped `FlowDefinition` into a flat instruction list (the Plan).
//!
//! Resuming a run then needs only a program counter, which is trivial to persist.

use std::{collections::HashMap, fmt};

use weft_domain::flow::{FlowDefinition, Step, StepKind};
use weft_expr::Condition;

/// Index into `Plan::instrs`.
pub type Pc = usize;

/// One instruction. Every instruction that came from a step carries its key for tracing.
#[derive(Debug, Clone, PartialEq)]
pub enum Instr {
    Action {
        step: String,
    },
    Branch {
        step: String,
        arms: Vec<Arm>,
        otherwise: Pc,
    },
    Jump {
        target: Pc,
    },
    Delay {
        step: String,
    },
    SetVar {
        step: String,
    },
    Stop {
        step: String,
    },
    End,
}

/// One decision branch: jump to `target` when `condition` matches.
#[derive(Debug, Clone, PartialEq)]
pub struct Arm {
    pub branch: String,
    pub condition: Condition,
    pub target: Pc,
}

/// A compiled flow version. Immutable; cache it per flow version.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub instrs: Vec<Instr>,
    steps: HashMap<String, Step>,
}

impl Plan {
    /// The step definition behind an instruction.
    #[must_use]
    pub fn step(&self, key: &str) -> Option<&Step> {
        self.steps.get(key)
    }

    /// Program counter of the instruction for `key` (actions, delays, …), if any.
    #[must_use]
    pub fn pc_of(&self, key: &str) -> Option<Pc> {
        self.instrs.iter().position(|i| instr_step(i) == Some(key))
    }
}

fn instr_step(instr: &Instr) -> Option<&str> {
    match instr {
        Instr::Action { step }
        | Instr::Branch { step, .. }
        | Instr::Delay { step }
        | Instr::SetVar { step }
        | Instr::Stop { step } => Some(step),
        Instr::Jump { .. } | Instr::End => None,
    }
}

/// Compilation failed (only for definitions that would also fail validation).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CompileError {
    #[error("duplicate step key `{0}`")]
    DuplicateKey(String),
}

/// Compiles a definition into a plan.
pub fn compile(def: &FlowDefinition) -> Result<Plan, CompileError> {
    let mut c = Compiler {
        instrs: Vec::new(),
        steps: HashMap::new(),
    };
    c.steps(&def.steps)?;
    c.instrs.push(Instr::End);
    Ok(Plan {
        instrs: c.instrs,
        steps: c.steps,
    })
}

struct Compiler {
    instrs: Vec<Instr>,
    steps: HashMap<String, Step>,
}

impl Compiler {
    fn steps(&mut self, steps: &[Step]) -> Result<(), CompileError> {
        for step in steps {
            if self.steps.insert(step.key.clone(), step.clone()).is_some() {
                return Err(CompileError::DuplicateKey(step.key.clone()));
            }
            let key = step.key.clone();
            match &step.kind {
                StepKind::Action(_) => self.instrs.push(Instr::Action { step: key }),
                StepKind::Delay(_) => self.instrs.push(Instr::Delay { step: key }),
                StepKind::SetVariable(_) => self.instrs.push(Instr::SetVar { step: key }),
                StepKind::Stop(_) => self.instrs.push(Instr::Stop { step: key }),
                StepKind::Decision(d) => {
                    let branch_pc = self.instrs.len();
                    self.instrs.push(Instr::End); // placeholder, patched below
                    let mut arms = Vec::with_capacity(d.branches.len());
                    let mut exits = Vec::new();
                    for branch in &d.branches {
                        let target = self.instrs.len();
                        self.steps(&branch.steps)?;
                        exits.push(self.instrs.len());
                        self.instrs.push(Instr::Jump { target: 0 });
                        arms.push(Arm {
                            branch: branch.key.clone(),
                            condition: branch.condition.clone(),
                            target,
                        });
                    }
                    let otherwise = self.instrs.len();
                    self.steps(&d.otherwise)?;
                    let end = self.instrs.len();
                    for exit in exits {
                        self.instrs[exit] = Instr::Jump { target: end };
                    }
                    self.instrs[branch_pc] = Instr::Branch {
                        step: key,
                        arms,
                        otherwise,
                    };
                }
            }
        }
        Ok(())
    }
}

impl fmt::Display for Plan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (pc, instr) in self.instrs.iter().enumerate() {
            match instr {
                Instr::Action { step } => writeln!(f, "{pc:<3}ACTION  {step}")?,
                Instr::Branch {
                    step,
                    arms,
                    otherwise,
                } => {
                    let arms: Vec<String> = arms
                        .iter()
                        .map(|a| format!("{}→{}", a.branch, a.target))
                        .collect();
                    writeln!(
                        f,
                        "{pc:<3}BRANCH  {step}  {}  otherwise→{otherwise}",
                        arms.join(" ")
                    )?;
                }
                Instr::Jump { target } => writeln!(f, "{pc:<3}JUMP    {target}")?,
                Instr::Delay { step } => writeln!(f, "{pc:<3}DELAY   {step}")?,
                Instr::SetVar { step } => writeln!(f, "{pc:<3}SETVAR  {step}")?,
                Instr::Stop { step } => writeln!(f, "{pc:<3}STOP    {step}")?,
                Instr::End => writeln!(f, "{pc:<3}END")?,
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn compiles_decisions_into_jumps() {
        let def: FlowDefinition = serde_json::from_value(json!({
            "trigger": {"type": "manual"},
            "steps": [
                {"key": "is_big", "type": "decision", "branches": [
                    {"key": "big", "condition": {"left": "1", "op": "eq", "right": "1"},
                     "steps": [{"key": "notify", "type": "action", "connector": "c", "action": "a"}]}
                ], "otherwise": [
                    {"key": "pretty", "type": "action", "connector": "c", "action": "a"},
                    {"key": "thank", "type": "action", "connector": "c", "action": "a"}
                ]},
                {"key": "wait", "type": "delay", "seconds": 60},
                {"key": "ask", "type": "action", "connector": "c", "action": "a"}
            ]
        }))
        .unwrap();
        let plan = compile(&def).unwrap();
        assert_eq!(
            plan.to_string(),
            "\
0  BRANCH  is_big  big→1  otherwise→3
1  ACTION  notify
2  JUMP    5
3  ACTION  pretty
4  ACTION  thank
5  DELAY   wait
6  ACTION  ask
7  END
"
        );
        assert_eq!(plan.pc_of("thank"), Some(4));
    }
}
