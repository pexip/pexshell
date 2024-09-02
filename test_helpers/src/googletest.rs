use googletest::{
    matcher::{Matcher, MatcherResult},
    prelude::MatcherBase,
};

pub fn debugs_as<T: std::fmt::Debug, MatcherT: for<'a> Matcher<&'a str>>(
    inner: MatcherT,
) -> impl for<'a> Matcher<&'a T> {
    DebugMatcher::<T, _> {
        inner,
        _phantom_t: std::marker::PhantomData,
    }
}

#[derive(MatcherBase)]
pub struct DebugMatcher<T: std::fmt::Debug, InnerMatcher> {
    inner: InnerMatcher,
    _phantom_t: std::marker::PhantomData<T>,
}

impl<T: std::fmt::Debug, InnerMatcher: for<'a> Matcher<&'a str>> Matcher<&T>
    for DebugMatcher<T, InnerMatcher>
{
    fn matches(&self, actual: &T) -> googletest::matcher::MatcherResult {
        self.inner.matches(&format!("{actual:?}"))
    }

    fn explain_match(&self, actual: &T) -> googletest::description::Description {
        format!(
            "which debugs as a string {}",
            self.inner.explain_match(&format!("{actual:?}"))
        )
        .into()
    }

    fn describe(
        &self,
        matcher_result: googletest::matcher::MatcherResult,
    ) -> googletest::description::Description {
        match matcher_result {
            MatcherResult::Match => format!(
                "debugs as a string which {}",
                self.inner.describe(MatcherResult::Match)
            )
            .into(),
            MatcherResult::NoMatch => format!(
                "doesn't debug as a string which {}",
                self.inner.describe(MatcherResult::Match)
            )
            .into(),
        }
    }
}
