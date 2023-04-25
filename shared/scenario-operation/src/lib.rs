use std::{ops::AddAssign, str::FromStr};

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

macro_rules! define_assignment_field {
    ($enum_name:ident{$($enum_item:ident($type:ty)),+$(,)?}) => {
        #[derive(Debug, serde::Deserialize, serde::Serialize)]
        #[serde(rename_all = "camelCase")]
        pub enum $enum_name {
            $(
                $enum_item($type),
            )+
        }

        pub mod pair {
            pub enum $enum_name<'a> {
                $(
                    $enum_item(&'a $type, &'a $type),
                )+
            }
        }

        pub mod mutable {
            #[derive(Debug)]
            pub enum $enum_name<'a> {
                $(
                    $enum_item(&'a mut $type),
                )+
            }
        }

        pub mod vec {
            #[derive(Debug)]
            pub enum $enum_name {
                $(
                    $enum_item(Vec<$type>),
                )+
            }
        }

        impl From<&$enum_name> for vec::$enum_name {
            fn from(value: &$enum_name) -> Self {
                match value {
                    $(
                        $enum_name::$enum_item(v) => Self::$enum_item(vec![*v]),
                    )+
                }
            }
        }

        pub trait Extract {
            fn extract(&self, v: &$enum_name) -> $enum_name;
            fn extract_mut(&mut self, v: &$enum_name) -> mutable::$enum_name;
        }

        impl $enum_name {
            pub fn zip<'a>(&'a self, other: &'a Self) -> Option<pair::$enum_name<'a>> {
                match (self, other) {
                    $(
                        (Self::$enum_item(v), Self::$enum_item(w)) => Some(pair::$enum_name::$enum_item(v, w)),
                    )+
                    _ => None
                }
            }
        }

        trait Delta {
            fn delta(s: &Self, t: &Self, n: u32) -> Self;
        }

        fn linear_space<T: Clone + AddAssign<T> + Delta>(u: &T, v: &T, k: &u32) -> Vec<T> {
            let n = k + 1;
            let d = Delta::delta(u, v, n);
            (0..n)
                .scan(u.clone(), |s, _| {
                    *s += d.clone();
                    Some(s.clone())
                })
                .collect()
        }

        #[derive(Debug, serde::Deserialize, serde::Serialize)]
        #[serde(rename_all = "camelCase")]
        pub enum Assignment {
            Immediate($enum_name),
            Linear($enum_name, u32),
        }

        impl Assignment {
            pub fn expand<E: Extract>(&self, env: &E) -> vec::$enum_name {
                match self {
                    Self::Immediate(v) => v.into(),
                    Self::Linear(v, k) => {
                        match env.extract(v).zip(v).unwrap() {
                            $(
                                pair::$enum_name::$enum_item(u, v) => vec::$enum_name::$enum_item(linear_space(u, v, k)),
                            )+
                        }
                    }
                }
            }
        }
    };
}

#[macro_export]
macro_rules! impl_extract {
    ($enum_name:ident -> $type:ty[$self_:ident] {
        $(
            $name:ident => $variable:expr
        ),+$(,)?
    }) => {
        impl Extract for $type {
            fn extract(&$self_, v: &$enum_name) -> $enum_name {
                match v {$(
                    $enum_name::$name(_) => $enum_name::$name($variable),
                )+}
            }

            fn extract_mut(&mut $self_, v: &$enum_name) -> $crate::mutable::$enum_name {
                match v {$(
                    $enum_name::$name(_) => $crate::mutable::$enum_name::$name(&mut $variable),
                )+}
            }
        }
    };
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

// #[derive(Debug, serde::Deserialize, serde::Serialize)]
// #[serde(rename_all = "camelCase")]
// pub enum AssignmentField {
//     GatheringFrequency(f64),
//     VaccinePerformRate(f64),
// }

// impl<T> Assignment<T>
// where
//     T: Copy + Sub<Output = T> + AddAssign<T>,
// {
//     pub fn atomic<E>(&self, env: &E) -> Vec<T> {
//         match self {
//             Assignment::Immediate(v) => vec![*v],
//             Assignment::Linear(v, k) => {
//                 let n = k + 1;
//                 (0..n)
//                 .scan(c, |s, _| {
//                     *s += c - *v;
//                     Some(*s)
//                 })
//                 .collect()
//             },
//         }
//     }
// }

macro_rules! impl_delta {
    ($num_type:ty) => {
        impl Delta for $num_type {
            fn delta(s: &Self, t: &Self, n: u32) -> Self {
                (*t - *s) / n as Self
            }
        }
    };
}

impl_delta!(f64);
impl_delta!(f32);
impl_delta!(u32);
impl_delta!(i32);

define_assignment_field!(
    AssignmentField {
        GatheringFrequency(f64),
        VaccinePerformRate(f64),
    }
);

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
    pub assignments: Vec<Assignment>,
}

impl Operation {
    pub fn new(condition: Condition, assignments: Vec<Assignment>) -> Self {
        Self {
            condition,
            assignments,
        }
    }
}
