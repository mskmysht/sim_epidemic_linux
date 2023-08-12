use std::str::FromStr;

use nom::{
    branch::alt,
    bytes::complete::tag,
    character::complete::{space0, space1},
    combinator::{eof, value},
    error::{Error, ParseError},
    multi::fold_many0,
    sequence::{delimited, pair, preceded, terminated},
    Finish, IResult, Parser,
};

#[derive(Debug)]
pub struct Predicate<Field>(Expr<Field>);

impl<Field> Predicate<Field>
where
    Field: FieldCombinator + PartialEq + PartialOrd,
{
    pub fn eval<E>(&self, env: &E) -> bool
    where
        E: EvalField<Field>,
    {
        eval_expr(&self.0, env)
    }
}

impl<Field> FromStr for Predicate<Field>
where
    Field: FieldCombinator,
{
    type Err = Error<String>;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match parse_expr(s).finish() {
            Ok((_, e)) => Ok(Self(e)),
            Err(e) => Err(Error::new(e.input.to_string(), e.code)),
        }
    }
}

fn eval_op<T: PartialEq + PartialOrd>(opt: &Operator, lhs: T, rhs: T) -> bool {
    match opt {
        Operator::Eq => lhs == rhs,
        Operator::Lt => lhs < rhs,
        Operator::Le => lhs <= rhs,
        Operator::Gt => lhs > rhs,
        Operator::Ge => lhs >= rhs,
    }
}

fn eval_expr<Field, E>(e: &Expr<Field>, env: &E) -> bool
where
    Field: FieldCombinator + PartialEq + PartialOrd,
    E: EvalField<Field>,
{
    match e {
        Expr::And(e1, e2) => eval_expr(e1, env) && eval_expr(e2, env),
        Expr::Or(e1, e2) => eval_expr(e1, env) || eval_expr(e2, env),
        Expr::BiRel(opt, opd) => {
            let (lhs, rhs) = env.eval(opd);
            eval_op(opt, &lhs, rhs)
        }
    }
}

pub trait EvalField<Field> {
    fn eval<'a>(&self, field: &'a Field) -> (Field, &'a Field);
}

#[derive(Debug)]
enum Expr<Field> {
    And(Box<Expr<Field>>, Box<Expr<Field>>),
    Or(Box<Expr<Field>>, Box<Expr<Field>>),
    BiRel(Operator, Field),
}

#[derive(Debug, Clone)]
pub enum Operator {
    Eq,
    Lt,
    Le,
    Gt,
    Ge,
}

fn parse_operator(input: &str) -> IResult<&str, Operator> {
    alt((
        value(Operator::Eq, tag("==")),
        value(Operator::Le, tag("<=")),
        value(Operator::Lt, tag("<")),
        value(Operator::Ge, tag(">=")),
        value(Operator::Gt, tag(">")),
    ))(input)
}

pub fn binary_relation<'a, F, G, O, Field>(
    word: &'a str,
    operator: F,
    operand: G,
) -> impl Parser<&'a str, (O, Field), Error<&'a str>>
where
    F: Parser<&'a str, O, Error<&'a str>>,
    G: Parser<&'a str, Field, Error<&'a str>>,
{
    preceded(
        tag(word),
        pair(delimited(space0, operator, space0), operand),
    )
}

pub trait FieldCombinator: Sized {
    fn combinator<'a, F, O>(operator: F, i: &'a str) -> IResult<&'a str, (O, Self)>
    where
        F: Parser<&'a str, O, Error<&'a str>>;
}

fn parse_birel<Field: FieldCombinator>(i: &str) -> IResult<&str, Expr<Field>> {
    let (i, (op, f)) = Field::combinator(parse_operator, i)?;
    Ok((i, Expr::BiRel(op, f)))
}

fn fold_expr<'a, E, F, A, Field>(
    op: &'a str,
    mut expr: F,
    f: A,
    i: &'a str,
) -> IResult<&'a str, Expr<Field>, E>
where
    E: ParseError<&'a str>,
    F: Parser<&'a str, Expr<Field>, E>,
    A: Fn(Box<Expr<Field>>, Box<Expr<Field>>) -> Expr<Field>,
{
    let (i, e1) = expr.parse(i)?;
    let (i, es) = fold_many0(
        preceded(delimited(space1, tag(op), space1), expr),
        Vec::new,
        |mut acc, e| {
            acc.push(e);
            acc
        },
    )(i)?;
    Ok((
        i,
        es.into_iter()
            .fold(e1, |acc, e| f(Box::new(acc), Box::new(e))),
    ))
}

fn term<Field: FieldCombinator>(i: &str) -> IResult<&str, Expr<Field>> {
    alt((
        parse_birel,
        delimited(pair(tag("("), space0), or_expr, pair(space0, tag(")"))),
    ))(i)
}

fn or_expr<Field: FieldCombinator>(i: &str) -> IResult<&str, Expr<Field>> {
    fold_expr("OR", and_expr, Expr::Or, i)
}

fn and_expr<Field: FieldCombinator>(i: &str) -> IResult<&str, Expr<Field>> {
    fold_expr("AND", term, Expr::And, i)
}

fn parse_expr<Field: FieldCombinator>(input: &str) -> IResult<&str, Expr<Field>> {
    terminated(delimited(space0, or_expr, space0), eof)(input)
}

#[cfg(test)]
mod tests {
    use nom::{
        branch::alt, character::complete::u32, combinator::map, error::Error, IResult, Parser,
    };

    use crate::{binary_relation, EvalField, FieldCombinator, Predicate};

    #[derive(Debug, PartialEq, PartialOrd)]
    pub enum CondField {
        Days(u32),
    }

    impl FieldCombinator for CondField {
        fn combinator<'a, F, O>(operator: F, i: &'a str) -> IResult<&'a str, (O, Self)>
        where
            F: Parser<&'a str, O, Error<&'a str>>,
        {
            alt((binary_relation("days", operator, map(u32, CondField::Days)),))(i)
        }
    }

    struct Env {
        days: u32,
    }

    impl EvalField<CondField> for Env {
        fn eval<'a>(&self, field: &'a CondField) -> (CondField, &'a CondField) {
            match field {
                CondField::Days(_) => (CondField::Days(self.days), field),
            }
        }
    }

    #[test]
    fn test_parser() {
        let env = Env { days: 3 };
        assert_eq!(
            "days == 1 OR days > 2"
                .parse::<Predicate<CondField>>()
                .unwrap()
                .eval(&env),
            env.days == 1 || env.days > 2
        );
        assert_eq!(
            "days <= 10 AND days == 5"
                .parse::<Predicate<CondField>>()
                .unwrap()
                .eval(&env),
            env.days <= 10 && env.days == 5
        );
        assert_eq!(
            "days <= 10 AND (days == 5 OR days < 4)"
                .parse::<Predicate<CondField>>()
                .unwrap()
                .eval(&env),
            env.days <= 10 && (env.days == 5 || env.days < 4)
        );
        assert_eq!(
            " ( days <= 10 AND days == 5 ) OR days < 4 "
                .parse::<Predicate<CondField>>()
                .unwrap()
                .eval(&env),
            (env.days <= 10 && env.days == 5) || env.days < 4
        );
        assert_eq!(
            "days <= 10 AND days == 5 OR days < 100 AND days > 4"
                .parse::<Predicate<CondField>>()
                .unwrap()
                .eval(&env),
            env.days <= 10 && env.days == 5 || env.days < 100 && env.days > 4
        );
    }
}
