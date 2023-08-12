use std::str::FromStr;

use nom::{branch::alt, character::complete::u32, combinator::map, error::Error, IResult, Parser};

pub use predicate::EvalField;
use predicate::{binary_relation, FieldCombinator, Predicate};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Container<I, T> {
    pub index: I,
    pub value: T,
}

pub trait CloneWith<T> {
    fn clone_with(&self, value: T) -> Self;
}

impl<I: Clone, T> CloneWith<T> for Container<I, T> {
    fn clone_with(&self, value: T) -> Self {
        Container {
            index: self.index.clone(),
            value,
        }
    }
}

impl<T> CloneWith<T> for T {
    #[inline]
    fn clone_with(&self, value: T) -> Self {
        value
    }
}

pub trait Interpolate<U> {
    type Target;
    fn interpolate<C: FromIterator<Self::Target>>(from: &Self, to: &U, n: &u32) -> C;
}

impl<I, T, U> Interpolate<Container<I, U>> for T
where
    T: Interpolate<U>,
{
    type Target = T::Target;
    fn interpolate<C: FromIterator<Self::Target>>(from: &Self, to: &Container<I, U>, n: &u32) -> C {
        T::interpolate::<C>(from, &to.value, n)
    }
}

#[macro_export]
macro_rules! impl_primitive_interpolate {
    ($type:ty) => {
        impl Interpolate<$type> for $type {
            type Target = $type;
            fn interpolate<C: FromIterator<Self::Target>>(from: &Self, to: &$type, n: &u32) -> C {
                let d = (*to - *from) / (*n as $type);
                (0..*n)
                    .scan(from.clone(), move |s, _| {
                        *s += d.clone();
                        Some(s.clone())
                    })
                    .collect::<C>()
            }
        }
    };
}

pub trait Assign<T> {
    fn assign(to: &mut Self, value: T);
}

#[macro_export]
macro_rules! accessor {
    ($env:ident: $env_type:ty, $accessor:ident {
        $(
        $item:ident($v:ident) =>
            get { $get:expr }
            set $set:block
        )+
    }) => {
        impl $crate::Interpolate<$accessor> for $env_type {
            type Target = $accessor;
            fn interpolate<C: FromIterator<Self::Target>>($env: &Self, to: &$accessor, n: &u32) -> C {
                match to {$(
                    $accessor::$item($v) => {
                        $crate::Interpolate::interpolate::<Vec<_>>($get, $v, n)
                            .into_iter()
                            .map(|k| $accessor::$item($crate::CloneWith::clone_with($v, k)))
                            .collect::<C>()
                    },
                )+}
            }
        }

        impl $crate::Assign<$accessor> for $env_type {
            fn assign($env: &mut Self, value: $accessor) {
                match value {$(
                    $accessor::$item($v) => $set
                )+}
            }
        }
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Assignment<T> {
    Value(T),
    Interpolate(T, u32),
}

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

#[derive(Debug)]
pub struct Condition<F>(Predicate<F>);

impl<F: FieldCombinator> FromStr for Condition<F> {
    type Err = <Predicate<F> as FromStr>::Err;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Condition(Predicate::from_str(s)?))
    }
}

impl<F: FieldCombinator + PartialEq + PartialOrd> Condition<F> {
    pub fn eval<E: EvalField<F>>(&self, env: &E) -> bool {
        self.0.eval(env)
    }
}

// implement (todo: extract above codes as a library)
impl_primitive_interpolate!(i32);
impl_primitive_interpolate!(u32);
impl_primitive_interpolate!(f32);
impl_primitive_interpolate!(f64);

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum VaccinationStrategy {
    PerformRate(f64),
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MyField {
    GatheringFrequency(f64),
    Vaccination(Container<usize, VaccinationStrategy>),
}

#[derive(Debug)]
pub struct Operation {
    pub condition: Condition<ConditionField>,
    pub assignments: Vec<Assignment<MyField>>,
}

impl Operation {
    pub fn new(
        condition: Condition<ConditionField>,
        assignments: Vec<Assignment<MyField>>,
    ) -> Self {
        Self {
            condition,
            assignments,
        }
    }
}
