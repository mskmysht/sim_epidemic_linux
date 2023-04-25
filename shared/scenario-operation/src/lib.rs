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

trait Delta {
    fn delta(s: &Self, t: &Self, n: u32) -> Self;
}

fn linear_space<T: Clone + AddAssign<T> + Delta, U, F: Fn(T) -> U>(
    u: &T,
    v: &T,
    k: &u32,
    f: F,
) -> std::collections::VecDeque<U> {
    let n = k + 1;
    let d = Delta::delta(u, v, n);
    (0..n)
        .scan(u.clone(), |s, _| {
            *s += d.clone();
            Some(f(s.clone()))
        })
        .collect()
}

pub trait Getter<T> {
    fn get(&self, item: &T) -> T;
}

pub trait Setter<T> {
    fn set(&mut self, item: T);
}

macro_rules! define_assignment_field {
    ($enum_name:ident{$($enum_item:ident($type:ty)),+$(,)?}) => {
        #[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
        #[serde(rename_all = "camelCase")]
        pub enum $enum_name {
            $(
                $enum_item($type),
            )+
        }

        impl $enum_name {
            pub fn assign<E: $crate::Setter<$enum_name>>(self, env: &mut E) {
                env.set(self);
            }
        }

        pub type AssignmentQueue = std::collections::VecDeque<$enum_name>;

        #[derive(Debug, serde::Deserialize, serde::Serialize)]
        #[serde(rename_all = "camelCase")]
        pub enum Assignment {
            Immediate($enum_name),
            Linear($enum_name, u32),
        }

        impl Assignment {
            pub fn expand<E: Getter<$enum_name>>(&self, env: &E) -> std::collections::VecDeque<$enum_name> {
                match self {
                    Self::Immediate(item) => {
                        let mut vs = std::collections::VecDeque::with_capacity(1);
                        vs.push_front(item.clone());
                        vs
                    },
                    Self::Linear(item, k) => {
                        match (item, env.get(item)) {
                            $(
                                ($enum_name::$enum_item(u), $enum_name::$enum_item(v)) => linear_space(u, &v, k, $enum_name::$enum_item),
                            )+
                            _ => unreachable!()
                        }
                    }
                }
            }
        }
    };
}

#[macro_export]
macro_rules! impl_accessor {
    ($self_:ident: $env:ty; $enum_name:ident {
        $(
            $name:ident =>
                get {$get:expr}
                set($v:ident) {$set:expr;}
        )+
    }) => {
        impl $crate::Getter<$enum_name> for $env {
            fn get(&$self_, item: &$enum_name) -> $enum_name {
                match item {
                    $(
                        $enum_name::$name(_) => $enum_name::$name($get),
                    )+
                }
            }
        }

        impl $crate::Setter<$enum_name> for $env {
            fn set(&mut $self_, item: $enum_name) {
                match item {
                    $(
                        $enum_name::$name($v) => {
                            $set
                        },
                    )+
                }
            }
        }
    };
}

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
