use googletest::matcher::{Matcher, MatcherResult};

pub fn debugs_as<T: std::fmt::Debug, MatcherT: Matcher<ActualT = String>>(
    inner: MatcherT,
) -> impl Matcher<ActualT = T> {
    DebugMatcher::<T, _> {
        inner,
        _phantom_t: std::marker::PhantomData,
    }
}

pub struct DebugMatcher<T: std::fmt::Debug, InnerMatcher: Matcher<ActualT = String>> {
    inner: InnerMatcher,
    _phantom_t: std::marker::PhantomData<T>,
}

impl<T: std::fmt::Debug, InnerMatcher: Matcher<ActualT = String>> Matcher
    for DebugMatcher<T, InnerMatcher>
{
    type ActualT = T;

    fn matches(&self, actual: &Self::ActualT) -> googletest::matcher::MatcherResult {
        self.inner.matches(&format!("{actual:?}"))
    }

    fn explain_match(&self, actual: &Self::ActualT) -> googletest::description::Description {
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
