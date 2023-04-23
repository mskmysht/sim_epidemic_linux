use std::{
    ops::{AddAssign, Sub},
    str::FromStr,
};

use nom::{branch::alt, character::complete::u32, combinator::map, error::Error, IResult, Parser};

pub use predicate::EvalField;
use predicate::{binary_relation, FieldCombinator, Predicate};

#[derive(Debug, PartialEq, PartialOrd)]
pub enum ConditionField {
    Days(u32),
}

impl FieldCombinator for ConditionField {
    fn combinator<'a, F, O>(operator: F, i: &'a str) -> IResult<&'a str, (O, Self)>
    where
        F: Parser<&'a str, O, Error<&'a str>>,
    {
        alt((binary_relation(
            "days",
            operator,
            map(u32, ConditionField::Days),
        ),))(i)
    }
}

// pub struct Env {
//     days: u32,
// }

// impl EvalField<ConditionField> for Env {
//     fn eval<'a>(&self, field: &'a ConditionField) -> (ConditionField, &'a ConditionField) {
//         match field {
//             ConditionField::Days(_) => (ConditionField::Days(self.days), field),
//         }
//     }
// }

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Assignment<T> {
    Immediate(T),
    Linear(T, u32),
}

impl<T> Assignment<T>
where
    T: Copy + Sub<Output = T> + AddAssign<T>,
{
    pub fn atom(a: &Assignment<T>, c: T, n: u32) -> Vec<T> {
        match a {
            Assignment::Immediate(v) => vec![*v],
            Assignment::Linear(v, g) => (n..=*g)
                .scan(c, |s, _| {
                    *s += c - *v;
                    Some(*s)
                })
                .collect(),
        }
    }
}

trait Assign<'de>: std::fmt::Debug + serde::Deserialize<'de> {
    type Value;
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AssignmentField {
    GatheringFrequency(f64),
    VaccinePerformRate(f64),
}

#[derive(Debug)]
pub struct Condition(Predicate<ConditionField>);

impl FromStr for Condition {
    type Err = <Predicate<ConditionField> as FromStr>::Err;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Condition(Predicate::from_str(s)?))
    }
}

impl Condition {
    pub fn eval<E: EvalField<ConditionField>>(&self, env: &E) -> bool {
        self.0.eval(env)
    }
}

#[derive(Debug)]
pub struct Operation {
    pub condition: Condition,
    pub assignments: Vec<AssignmentField>,
}

impl Operation {
    pub fn new(condition: Condition, assignments: Vec<AssignmentField>) -> Self {
        Self {
            condition,
            assignments,
        }
    }
}
