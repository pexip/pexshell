use crate::util::SensitiveString;
use googletest::prelude::*;

pub fn sensitive_string<
    MatcherT: for<'a> Matcher<&'a SensitiveStringWrapper>,
    T: AsRef<SensitiveString> + std::fmt::Debug,
>(
    inner: MatcherT,
) -> SensitiveStringMatcher<T, MatcherT> {
    SensitiveStringMatcher {
        inner,
        _phantom_t: std::marker::PhantomData,
    }
}

#[derive(MatcherBase)]
pub struct SensitiveStringMatcher<T, InnerMatcherT> {
    inner: InnerMatcherT,
    _phantom_t: std::marker::PhantomData<T>,
}

pub struct SensitiveStringWrapper(SensitiveString);

impl std::fmt::Debug for SensitiveStringWrapper {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.secret().fmt(f)
    }
}

impl PartialEq<str> for SensitiveStringWrapper {
    fn eq(&self, other: &str) -> bool {
        self.0.secret() == other
    }
}

impl<'a> PartialEq<&'a str> for SensitiveStringWrapper {
    fn eq(&self, other: &&'a str) -> bool {
        self.0.secret() == *other
    }
}

impl<MatcherT, T: AsRef<SensitiveString> + std::fmt::Debug> Matcher<&T>
    for SensitiveStringMatcher<T, MatcherT>
where
    MatcherT: for<'a> Matcher<&'a SensitiveStringWrapper>,
{
    fn matches(&self, actual: &T) -> googletest::matcher::MatcherResult {
        self.inner
            .matches(&SensitiveStringWrapper(actual.as_ref().clone()))
    }

    fn describe(
        &self,
        matcher_result: googletest::matcher::MatcherResult,
    ) -> googletest::description::Description {
        self.inner.describe(matcher_result)
    }

    fn explain_match(&self, actual: &T) -> googletest::description::Description {
        self.inner
            .explain_match(&SensitiveStringWrapper(actual.as_ref().clone()))
    }
}
