use crate::concurrent::{handle_thread_r_requests, PolarsBackgroundHandle};
use crate::lazy::dsl::*;
use crate::rdatatype::new_join_type;
use crate::rdatatype::new_quantile_interpolation_option;
use crate::rdatatype::new_unique_keep_strategy;
use crate::robj_to;
use crate::utils::{r_result_list, try_f64_into_u32, try_f64_into_usize};
use extendr_api::prelude::*;
use polars::prelude as pl;

#[allow(unused_imports)]
use std::result::Result;

#[derive(Clone)]
pub struct LazyFrame(pub pl::LazyFrame);

impl From<pl::LazyFrame> for LazyFrame {
    fn from(item: pl::LazyFrame) -> Self {
        LazyFrame(item)
    }
}

#[extendr]
impl LazyFrame {
    fn print(&self) -> Self {
        rprintln!("{}", self.0.describe_plan());
        self.clone()
    }

    pub fn describe_plan(&self) {
        rprintln!("{}", self.0.describe_plan());
    }

    pub fn describe_optimized_plan(&self) -> List {
        let result = self.0.describe_optimized_plan().map(|opt_plan| {
            rprintln!("{}", opt_plan);
        });
        r_result_list(result.map_err(|err| format!("{:?}", err)))
    }

    pub fn collect_background(&self) -> PolarsBackgroundHandle {
        PolarsBackgroundHandle::new(self)
    }

    pub fn collect(&self) -> List {
        let result = handle_thread_r_requests(self.clone().0).map_err(|err| {
            //improve err messages
            let err_string = match err {
                pl::PolarsError::InvalidOperation(polars::error::ErrString::Owned(x)) => {
                    format!("Something (Likely a Column) named {:?} was not found", x)
                }
                x => format!("{:?}", x),
            };

            format!("when calling $collect() on LazyFrame:\n{}", err_string)
        });
        r_result_list(result)
    }

    fn first(&self) -> Self {
        self.0.clone().first().into()
    }

    fn last(&self) -> Self {
        self.0.clone().last().into()
    }

    fn max(&self) -> Self {
        self.0.clone().max().into()
    }

    fn min(&self) -> Self {
        self.0.clone().min().into()
    }

    fn mean(&self) -> Self {
        self.0.clone().mean().into()
    }

    fn median(&self) -> Self {
        self.0.clone().median().into()
    }

    fn sum(&self) -> Self {
        self.0.clone().sum().into()
    }

    pub fn std(&self, ddof: Robj) -> Result<Self, String> {
        Ok(self.clone().0.std(robj_to!(u8, ddof)?).into())
    }

    pub fn var(&self, ddof: Robj) -> Result<Self, String> {
        Ok(self.clone().0.var(robj_to!(u8, ddof)?).into())
    }

    pub fn quantile(&self, quantile: Robj, interpolation: Robj) -> Result<Self, String> {
        let res = new_quantile_interpolation_option(robj_to!(str, interpolation)?).unwrap();
        Ok(self
            .clone()
            .0
            .quantile(robj_to!(Expr, quantile)?.0.clone(), res)
            .into())
    }

    fn shift(&self, periods: Robj) -> Result<Self, String> {
        Ok(self.clone().0.shift(robj_to!(i64, periods)?).into())
    }

    fn shift_and_fill(&self, fill_value: Robj, periods: Robj) -> Result<Self, String> {
        Ok(self
            .clone()
            .0
            .shift_and_fill(robj_to!(i64, periods)?, robj_to!(Expr, fill_value)?.0)
            .into())
    }

    fn reverse(&self) -> Self {
        self.0.clone().reverse().into()
    }

    fn drop(&self, columns: Robj) -> Result<LazyFrame, String> {
        Ok(self
            .0
            .clone()
            .drop_columns(robj_to!(Vec, String, columns)?)
            .into())
    }

    fn fill_nan(&self, fill_value: Robj) -> Result<Self, String> {
        Ok(self
            .0
            .clone()
            .fill_nan(robj_to!(Expr, fill_value)?.0)
            .into())
    }

    fn fill_null(&self, fill_value: Robj) -> Result<Self, String> {
        Ok(self
            .0
            .clone()
            .fill_null(robj_to!(Expr, fill_value)?.0)
            .into())
    }

    fn slice(&self, offset: Robj, length: Robj) -> Result<LazyFrame, String> {
        Ok(LazyFrame(self.0.clone().slice(
            robj_to!(i64, offset)?,
            robj_to!(Option, u32, length)?.unwrap_or(u32::MAX),
        )))
    }

    fn select(&self, exprs: &ProtoExprArray) -> LazyFrame {
        let exprs: Vec<pl::Expr> = exprs
            .0
            .iter()
            .map(|protoexpr| protoexpr.to_rexpr("select").0)
            .collect();

        let new_df = self.clone().0.select(exprs);

        LazyFrame(new_df)
    }

    fn limit(&self, n: Robj) -> Result<Self, String> {
        Ok(self.0.clone().limit(robj_to!(u32, n)?).into())
    }

    fn tail(&self, n: Robj) -> Result<LazyFrame, String> {
        Ok(LazyFrame(self.0.clone().tail(robj_to!(u32, n)?)))
    }

    fn filter(&self, expr: &Expr) -> LazyFrame {
        let new_df = self.clone().0.filter(expr.0.clone());
        LazyFrame(new_df)
    }

    fn drop_nulls(&self, subset: &ProtoExprArray) -> LazyFrame {
        if subset.0.len() == 0 {
            LazyFrame(self.0.clone().drop_nulls(None))
        } else {
            let vec = pra_to_vec(subset, "select");
            LazyFrame(self.0.clone().drop_nulls(Some(vec)))
        }
    }

    fn unique(&self, subset: Robj, keep: Robj) -> Result<LazyFrame, String> {
        let ke = new_unique_keep_strategy(robj_to!(str, keep)?).unwrap();
        Ok(if subset.len() == 0 {
            LazyFrame(self.0.clone().unique(None, ke))
        } else {
            LazyFrame(
                self.0
                    .clone()
                    .unique(Some(robj_to!(Vec, String, subset)?), ke),
            )
        })
    }

    fn groupby(&self, exprs: &ProtoExprArray, maintain_order: bool) -> LazyGroupBy {
        let expr_vec = pra_to_vec(exprs, "select");
        if maintain_order {
            LazyGroupBy(self.0.clone().groupby_stable(expr_vec))
        } else {
            LazyGroupBy(self.0.clone().groupby(expr_vec))
        }
    }

    fn with_columns(&self, exprs: &ProtoExprArray) -> LazyFrame {
        LazyFrame(self.0.clone().with_columns(pra_to_vec(exprs, "select")))
    }

    fn with_column(&self, expr: &Expr) -> LazyFrame {
        LazyFrame(self.0.clone().with_column(expr.0.clone()))
    }

    fn join(
        &self,
        other: &LazyFrame,
        left_on: &ProtoExprArray,
        right_on: &ProtoExprArray,
        how: &str,
        suffix: &str,
        allow_parallel: bool,
        force_parallel: bool,
    ) -> LazyFrame {
        let ldf = self.0.clone();
        let other = other.0.clone();
        let left_on = pra_to_vec(left_on, "select");
        let right_on = pra_to_vec(right_on, "select");
        let how = new_join_type(how);

        LazyFrame(
            ldf.join_builder()
                .with(other)
                .left_on(left_on)
                .right_on(right_on)
                .allow_parallel(allow_parallel)
                .force_parallel(force_parallel)
                .how(how)
                .suffix(suffix)
                .finish(),
        )
    }

    pub fn sort_by_exprs(
        &self,
        by: Robj,
        descending: Robj,
        nulls_last: Robj,
    ) -> Result<Self, String> {
        let ldf = self.0.clone();
        let exprs = robj_to!(VecPLExpr, by).map_err(|err| format!("the arg [...] or {}", err))?;
        let descending = robj_to!(Vec, bool, descending)?;
        let nulls_last = robj_to!(bool, nulls_last)?;
        Ok(ldf.sort_by_exprs(exprs, descending, nulls_last).into())
    }
}

#[derive(Clone)]
pub struct LazyGroupBy(pub pl::LazyGroupBy);

#[extendr]
impl LazyGroupBy {
    fn print(&self) {
        rprintln!(" The insides of this object is a mystery, inspect the lazyframe instead.");
    }

    fn agg(&self, exprs: &ProtoExprArray) -> LazyFrame {
        let expr_vec = pra_to_vec(exprs, "select");
        LazyFrame(self.0.clone().agg(expr_vec))
    }

    fn head(&self, n: f64) -> List {
        r_result_list(
            try_f64_into_usize(n)
                .map(|n| LazyFrame(self.0.clone().head(Some(n))))
                .map_err(|err| format!("head: {}", err)),
        )
    }

    fn tail(&self, n: f64) -> List {
        r_result_list(
            try_f64_into_usize(n)
                .map(|n| LazyFrame(self.0.clone().tail(Some(n))))
                .map_err(|err| format!("tail: {}", err)),
        )
    }

    // fn apply(&self, robj: Robj, val: f64) -> Robj {
    //     todo!("not done");
    // }
}

extendr_module! {
    mod dataframe;
    impl LazyFrame;
    impl LazyGroupBy;
}
