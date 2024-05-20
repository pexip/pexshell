use std::{collections::HashSet, fmt::Debug};

/// Indicates the result of a matching attempt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatchResult {
    /// Indicates the [`Expectation`] couldn't match to the [`log::Record`].
    /// This implies that the state of the [`Expectation`] has not changed.
    NotMatch,
    /// Indicates that the [`Expectation`] did match to the [`log::Record`] and that the state of the [`Expectation`]
    /// has changed, but is not yet completely matched.
    /// In practice, this often means the expectation is trying to match against multiple log messages and is still
    /// waiting for more.
    Match,
    /// Indicates that the [`Expectation`] did match to the [`log::Record`] and is completely fulfilled.
    Complete,
}

pub trait Expectation: Send + 'static + Debug {
    fn matches(&mut self, record: &log::Record) -> MatchResult;
    fn reset(&mut self);
}

/// Matches logs against a specific log level, module path and message. Must match exactly.
#[must_use]
pub fn exact(level: log::Level, module_path: &str, message: &str) -> impl Expectation {
    Exact {
        level,
        module_path: module_path.into(),
        message: message.into(),
    }
}

/// Matches logs of a specific log level.
#[must_use]
pub fn level(level: log::Level) -> impl Expectation {
    Level { level }
}

/// Matches log messages that contain the specified substring.
#[must_use]
pub fn contains(substring: &str) -> impl Expectation {
    Contains {
        substring: substring.into(),
    }
}

/// Matches log messages on a given predicate.
pub fn predicate(
    predicate: impl Fn(&log::Record) -> bool + Send + 'static,
    description: impl Into<String>,
) -> impl Expectation {
    Predicate {
        predicate: Box::new(predicate),
        description: description.into(),
    }
}

/// Matches log messages on a given predicate. Uses `stringify!` on the predicate to produce a description.
#[macro_export]
macro_rules! predicate {
    ( $x:expr ) => {{
        $crate::logging::expect::predicate($x, stringify!($x))
    }};
}

/// Matches if and only if all expectations completely match (`MatchResult::Complete`) on the given log message.
/// Partial matches (`MatchResult::Match`) are ignored.
#[must_use]
pub fn all(expectations: Vec<Box<dyn Expectation>>) -> impl Expectation {
    All { expectations }
}

/// Matches if and only if all expectations completely match (`MatchResult::Complete`) on the given log message.
/// Partial matches (`MatchResult::Match`) are ignored.
#[macro_export]
macro_rules! all {
    [ $( $x:expr ),* $(,)? ] => {
        {
            let b_vec: Vec<Box<dyn $crate::logging::expect::Expectation>> = vec![$(Box::new($x)),*];
            $crate::logging::expect::all(b_vec)
        }
    };
}

/// Matches if and only if any of the given expectations completely match (`MatchResult::Complete`) on the given log message.
/// Expectations are tried in order. Partial matches (`MatchResult::Match`) are ignored.
#[must_use]
pub fn any(expectations: Vec<Box<dyn Expectation>>) -> impl Expectation {
    Any { expectations }
}

/// Matches if and only if any of the given expectations completely match (`MatchResult::Complete`) on the given log message.
/// Expectations are tried in order. Partial matches (`MatchResult::Match`) are ignored.
#[macro_export]
macro_rules! any {
    [ $( $x:expr ),* $(,)? ] => {
        {
            let b_vec: Vec<Box<dyn $crate::logging::expect::Expectation>> = vec![$(Box::new($x)),*];
            $crate::logging::expect::any(b_vec)
        }
    }
}

/// Matches if all expectations completely match in the order they are given. Expectations do not all have to match
/// completely on a single message, but the next expectation cannot begin matching until the previous expectation is complete.
#[must_use]
pub fn in_order(expectations: Vec<Box<dyn Expectation>>) -> impl Expectation {
    InOrder {
        expectations,
        next: 0,
        previous: None,
    }
}

/// Matches if all expectations completely match in the order they are given. Expectations do not all have to match
/// completely on a single message, but the next expectation cannot begin matching until the previous expectation is complete.
/// Each log message can at most be matched against a single expectation, so two identical expectations in sequence
/// would consume one matching log message (or matching sequence of messages) each.
#[macro_export]
macro_rules! in_order {
    [ $( $x:expr ),* $(,)? ] => {
        {
            let b_vec: Vec<Box<dyn $crate::logging::expect::Expectation>> = vec![$(Box::new($x)),*];
            $crate::logging::expect::in_order(b_vec)
        }
    }
}

/// Matches if all expectations completely match. For each log, expectations are tried in the order they are given and
/// when an expectation is matched, subsequent expectations are not tried. Expectations do not all have to match
/// completely against any one log message.
#[must_use]
pub fn set(expectations: Vec<Box<dyn Expectation>>) -> impl Expectation {
    Set {
        expectations,
        complete: HashSet::new(),
        partial: HashSet::new(),
    }
}

/// Matches if all expectations completely match. For each log, expectations are tried in the order they are given and
/// when an expectation is matched, subsequent expectations are not tried. Expectations do not all have to match
/// completely against any one log message.
#[macro_export]
macro_rules! set {
    [ $( $x:expr ),* $(,)? ] => {
        {
            let b_vec: Vec<Box<dyn $crate::logging::expect::Expectation>> = vec![$(Box::new($x)),*];
            $crate::logging::expect::set(b_vec)
        }
    }
}

/// Matches if any expectation completely matches. For each log, expectations are tried in the order they are given and
/// when an expectation is matched, subsequent expectations are not tried. Expectations do not have to match
/// completely against any one log message.
#[must_use]
pub fn any_set(expectations: Vec<Box<dyn Expectation>>) -> impl Expectation {
    AnySet {
        expectations,
        partial: HashSet::new(),
        complete: false,
    }
}

/// Matches if any expectation completely matches. For each log, expectations are tried in the order they are given and
/// when an expectation is matched, subsequent expectations are not tried. Expectations do not have to match
/// completely against any one log message.
#[macro_export]
macro_rules! any_set {
    [ $( $x:expr ),* $(,)? ] => {
        {
            let b_vec: Vec<Box<dyn $crate::logging::expect::Expectation>> = vec![$(Box::new($x)),*];
            $crate::logging::expect::any_set(b_vec)
        }
    }
}

/// Matches if all expectations completely match. All expectations are tried on all log messages until all are complete.
/// Expectations do not have to match completely against any one log message.
#[must_use]
pub fn group(expectations: Vec<Box<dyn Expectation>>) -> impl Expectation {
    Group {
        expectations,
        complete: HashSet::new(),
        partial: HashSet::new(),
    }
}

#[macro_export]
macro_rules! group {
    [ $( $x:expr ),* $(,)? ] => {
        {
            let b_vec: Vec<Box<dyn $crate::logging::expect::Expectation>> = vec![$(Box::new($x)),*];
            $crate::logging::expect::group(b_vec)
        }
    }
}

#[must_use]
pub fn any_group(expectations: Vec<Box<dyn Expectation>>) -> impl Expectation {
    AnyGroup {
        expectations,
        partial: HashSet::new(),
        complete: false,
    }
}

#[macro_export]
macro_rules! any_group {
    [ $( $x:expr ),* $(,)? ] => {
        {
            let b_vec: Vec<Box<dyn $crate::logging::expect::Expectation>> = vec![$(Box::new($x)),*];
            $crate::logging::expect::any_group(b_vec)
        }
    }
}

struct Exact {
    level: log::Level,
    module_path: String,
    message: String,
}

impl Debug for Exact {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Exact")
            .field("level", &self.level)
            .field("module_path", &self.module_path)
            .field("message", &self.message)
            .finish()
    }
}

impl Expectation for Exact {
    fn matches(&mut self, record: &log::Record) -> MatchResult {
        if self.level == record.level()
            && Some(self.module_path.as_str()) == record.module_path()
            && self.message == record.args().to_string()
        {
            MatchResult::Complete
        } else {
            MatchResult::NotMatch
        }
    }
    fn reset(&mut self) {}
}

struct Level {
    level: log::Level,
}

impl Debug for Level {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Level").field("level", &self.level).finish()
    }
}

impl Expectation for Level {
    fn matches(&mut self, record: &log::Record) -> MatchResult {
        if self.level == record.level() {
            MatchResult::Complete
        } else {
            MatchResult::NotMatch
        }
    }

    fn reset(&mut self) {}
}

struct Contains {
    substring: String,
}

impl Debug for Contains {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Contains")
            .field("substring", &self.substring)
            .finish()
    }
}

impl Expectation for Contains {
    fn matches(&mut self, record: &log::Record) -> MatchResult {
        if record.args().to_string().contains(&self.substring) {
            MatchResult::Complete
        } else {
            MatchResult::NotMatch
        }
    }

    fn reset(&mut self) {}
}

struct Predicate {
    predicate: Box<dyn Fn(&log::Record) -> bool + Send>,
    description: String,
}

impl Debug for Predicate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Closure")
            .field("description", &self.description)
            .finish_non_exhaustive()
    }
}

impl Expectation for Predicate {
    fn matches(&mut self, record: &log::Record) -> MatchResult {
        if (*self.predicate)(record) {
            MatchResult::Complete
        } else {
            MatchResult::NotMatch
        }
    }

    fn reset(&mut self) {}
}

struct All {
    expectations: Vec<Box<dyn Expectation>>,
}

impl Debug for All {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("all")
            .field("expectations", &self.expectations)
            .finish()
    }
}

impl Expectation for All {
    fn matches(&mut self, record: &log::Record) -> MatchResult {
        for expectation in &mut self.expectations {
            match expectation.matches(record) {
                MatchResult::NotMatch => return MatchResult::NotMatch,
                MatchResult::Match => {
                    expectation.reset();
                    return MatchResult::NotMatch;
                }
                MatchResult::Complete => expectation.reset(),
            }
        }
        MatchResult::Complete
    }

    fn reset(&mut self) {}
}

struct Any {
    expectations: Vec<Box<dyn Expectation>>,
}

impl Debug for Any {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("any")
            .field("expectations", &self.expectations)
            .finish()
    }
}

impl Expectation for Any {
    fn matches(&mut self, record: &log::Record) -> MatchResult {
        for expectation in &mut self.expectations {
            match expectation.matches(record) {
                MatchResult::NotMatch => {}
                MatchResult::Match => expectation.reset(),
                MatchResult::Complete => {
                    expectation.reset();
                    return MatchResult::Complete;
                }
            }
        }
        MatchResult::NotMatch
    }

    fn reset(&mut self) {}
}

struct InOrder {
    expectations: Vec<Box<dyn Expectation>>,
    previous: Option<usize>,
    next: usize,
}

impl Debug for InOrder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InOrder")
            .field("expectations", &self.expectations)
            .field("previous", &self.previous)
            .field("next", &self.next)
            .finish()
    }
}

impl Expectation for InOrder {
    fn matches(&mut self, record: &log::Record) -> MatchResult {
        if self.next == self.expectations.len() {
            return MatchResult::NotMatch;
        }
        match self.expectations[self.next].matches(record) {
            MatchResult::NotMatch => MatchResult::NotMatch,
            MatchResult::Match => {
                self.previous = Some(self.next);
                self.next += 1;
                MatchResult::Match
            }
            MatchResult::Complete => {
                self.previous = Some(self.next);
                self.next += 1;
                if self.next == self.expectations.len() {
                    MatchResult::Complete
                } else {
                    MatchResult::Match
                }
            }
        }
    }

    fn reset(&mut self) {
        if let Some(previous) = self.previous {
            for i in 0..=previous {
                self.expectations[i].reset();
            }
            self.previous = None;
            self.next = 0;
        }
    }
}

struct Set {
    expectations: Vec<Box<dyn Expectation>>,
    partial: HashSet<usize>,
    complete: HashSet<usize>,
}

impl Debug for Set {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Set")
            .field(
                "unmet_expectations",
                &self
                    .expectations
                    .iter()
                    .enumerate()
                    .filter_map(|(i, e)| {
                        if self.complete.contains(&i) {
                            None
                        } else {
                            Some(e)
                        }
                    })
                    .collect::<Vec<_>>(),
            )
            .finish_non_exhaustive()
    }
}

impl Expectation for Set {
    fn matches(&mut self, record: &log::Record) -> MatchResult {
        for (i, expectation) in self.expectations.iter_mut().enumerate() {
            if self.complete.contains(&i) {
                continue;
            }
            match expectation.matches(record) {
                MatchResult::NotMatch => {}
                MatchResult::Match => {
                    self.partial.insert(i);
                    return MatchResult::Match;
                }
                MatchResult::Complete => {
                    self.partial.insert(i);
                    assert!(self.complete.insert(i));
                    if self.complete.len() == self.expectations.len() {
                        return MatchResult::Complete;
                    }
                    return MatchResult::Match;
                }
            }
        }
        MatchResult::NotMatch
    }

    fn reset(&mut self) {
        for &i in &self.partial {
            self.expectations[i].reset();
        }
        self.complete.clear();
        self.partial.clear();
    }
}

struct AnySet {
    expectations: Vec<Box<dyn Expectation>>,
    partial: HashSet<usize>,
    complete: bool,
}

impl Debug for AnySet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AnySet")
            .field("expectations", &self.expectations)
            .finish_non_exhaustive()
    }
}

impl Expectation for AnySet {
    fn matches(&mut self, record: &log::Record) -> MatchResult {
        if self.complete {
            return MatchResult::NotMatch;
        }
        for (i, expectation) in self.expectations.iter_mut().enumerate() {
            match expectation.matches(record) {
                MatchResult::NotMatch => {}
                MatchResult::Match => {
                    self.partial.insert(i);
                    return MatchResult::Match;
                }
                MatchResult::Complete => {
                    self.partial.insert(i);
                    self.complete = true;
                    return MatchResult::Complete;
                }
            }
        }
        MatchResult::NotMatch
    }

    fn reset(&mut self) {
        for &i in &self.partial {
            self.expectations[i].reset();
        }
        self.partial.clear();
        self.complete = false;
    }
}

struct Group {
    expectations: Vec<Box<dyn Expectation>>,
    partial: HashSet<usize>,
    complete: HashSet<usize>,
}

impl Debug for Group {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Group")
            .field(
                "unmet_expectations",
                &self
                    .expectations
                    .iter()
                    .enumerate()
                    .filter_map(|(i, e)| {
                        if self.complete.contains(&i) {
                            None
                        } else {
                            Some(e)
                        }
                    })
                    .collect::<Vec<_>>(),
            )
            .finish_non_exhaustive()
    }
}

impl Expectation for Group {
    fn matches(&mut self, record: &log::Record) -> MatchResult {
        if self.complete.len() == self.expectations.len() {
            return MatchResult::NotMatch;
        }
        let mut has_matched = false;
        for (i, expectation) in self.expectations.iter_mut().enumerate() {
            if self.complete.contains(&i) {
                continue;
            }
            match expectation.matches(record) {
                MatchResult::NotMatch => {}
                MatchResult::Match => {
                    self.partial.insert(i);
                    has_matched = true;
                }
                MatchResult::Complete => {
                    self.partial.insert(i);
                    assert!(self.complete.insert(i));
                    has_matched = true;
                }
            }
        }
        if has_matched {
            if self.complete.len() == self.expectations.len() {
                MatchResult::Complete
            } else {
                MatchResult::Match
            }
        } else {
            MatchResult::NotMatch
        }
    }

    fn reset(&mut self) {
        for &i in &self.partial {
            self.expectations[i].reset();
        }
        self.complete.clear();
        self.partial.clear();
    }
}

struct AnyGroup {
    expectations: Vec<Box<dyn Expectation>>,
    partial: HashSet<usize>,
    complete: bool,
}

impl Debug for AnyGroup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AnyGroup")
            .field("expectations", &self.expectations)
            .finish_non_exhaustive()
    }
}

impl Expectation for AnyGroup {
    fn matches(&mut self, record: &log::Record) -> MatchResult {
        if self.complete {
            return MatchResult::NotMatch;
        }
        let mut has_matched = false;
        for (i, expectation) in self.expectations.iter_mut().enumerate() {
            match expectation.matches(record) {
                MatchResult::NotMatch => {}
                MatchResult::Match => {
                    self.partial.insert(i);
                    has_matched = true;
                }
                MatchResult::Complete => {
                    self.partial.insert(i);
                    self.complete = true;
                    return MatchResult::Complete;
                }
            }
        }
        if has_matched {
            MatchResult::Match
        } else {
            MatchResult::NotMatch
        }
    }

    fn reset(&mut self) {
        for &i in &self.partial {
            self.expectations[i].reset();
        }
        self.complete = false;
        self.partial.clear();
    }
}

#[cfg(test)]
mod tests {
    use googletest::{assert_that, matchers::eq};
    use log::Record;
    use test_case::test_case;

    use super::*;

    #[test_case(Some(1), MatchResult::Complete)]
    #[test_case(Some(0), MatchResult::NotMatch)]
    #[test_case(Some(2), MatchResult::NotMatch)]
    #[test_case(None, MatchResult::NotMatch)]
    fn test_predicate(line: Option<u32>, match_result: MatchResult) {
        let mut predicate = predicate!(|x| x.line() == Some(1));
        let record = Record::builder().line(line).build();
        assert_that!(predicate.matches(&record), eq(match_result));
    }

    #[test_case(log::Level::Info, "mod", "message", MatchResult::Complete)]
    #[test_case(log::Level::Warn, "mod", "message", MatchResult::NotMatch)]
    #[test_case(log::Level::Info, "other_mod", "message", MatchResult::NotMatch)]
    #[test_case(log::Level::Info, "mod", "other message", MatchResult::NotMatch)]
    fn test_exact(level: log::Level, mod_path: &str, message: &str, match_result: MatchResult) {
        let record = Record::builder()
            .level(log::Level::Info)
            .module_path(Some("mod"))
            .args(format_args!("message"))
            .build();

        let mut exact = exact(level, mod_path, message);

        assert_that!(exact.matches(&record), eq(match_result));
    }

    #[test_case(
        Some("some_file.log"),
        Some(1),
        log::Level::Info,
        MatchResult::Complete
    )]
    #[test_case(None, None, log::Level::Trace, MatchResult::NotMatch)]
    #[test_case(
        Some("some_other_file.log"),
        Some(1),
        log::Level::Info,
        MatchResult::NotMatch
    )]
    fn test_all(
        file: Option<&str>,
        line: Option<u32>,
        level: log::Level,
        match_result: MatchResult,
    ) {
        let mut all = all!(
            predicate!(|x| x.file() == Some("some_file.log")),
            predicate!(|x| x.line() == Some(1)),
            self::level(log::Level::Info),
        );
        let record = Record::builder().file(file).line(line).level(level).build();

        assert_that!(all.matches(&record), eq(match_result));
    }

    #[allow(clippy::cognitive_complexity)]
    #[allow(clippy::too_many_lines)]
    #[test]
    fn test_sequences() {
        let r_info = Record::builder().level(log::Level::Info).build();
        let r_trace = Record::builder().level(log::Level::Trace).build();

        let mut all = all!(level(log::Level::Info), level(log::Level::Info));
        let mut all_set = all!(set!(level(log::Level::Info), level(log::Level::Info)));
        let mut any = any!(
            level(log::Level::Trace),
            set!(level(log::Level::Info), level(log::Level::Info)),
        );
        let mut in_order = in_order!(
            level(log::Level::Info),
            level(log::Level::Info),
            level(log::Level::Trace),
            level(log::Level::Info),
        );
        let mut set = set!(level(log::Level::Info), level(log::Level::Info));
        let mut any_set_a = any_set!(
            in_order!(
                level(log::Level::Info),
                level(log::Level::Trace),
                level(log::Level::Info)
            ),
            in_order!(level(log::Level::Info), level(log::Level::Trace)),
        );
        let mut any_set_b = any_set!(
            in_order!(level(log::Level::Info), level(log::Level::Trace)),
            in_order!(
                level(log::Level::Info),
                level(log::Level::Trace),
                level(log::Level::Info),
            ),
        );
        let mut group = group!(
            in_order!(level(log::Level::Info), level(log::Level::Trace)),
            in_order!(
                level(log::Level::Info),
                level(log::Level::Trace),
                level(log::Level::Info),
            ),
        );
        let mut any_group = any_group!(
            in_order!(
                level(log::Level::Info),
                level(log::Level::Trace),
                level(log::Level::Info),
            ),
            in_order!(level(log::Level::Info), level(log::Level::Trace)),
        );

        for _ in 0..3 {
            // First log line
            assert_that!(all.matches(&r_info), eq(MatchResult::Complete));
            assert_that!(all_set.matches(&r_info), eq(MatchResult::NotMatch));
            assert_that!(any.matches(&r_info), eq(MatchResult::NotMatch));
            assert_that!(in_order.matches(&r_info), eq(MatchResult::Match));
            assert_that!(set.matches(&r_info), eq(MatchResult::Match));
            assert_that!(any_set_a.matches(&r_info), eq(MatchResult::Match));
            assert_that!(any_set_b.matches(&r_info), eq(MatchResult::Match));
            assert_that!(group.matches(&r_info), eq(MatchResult::Match));
            assert_that!(any_group.matches(&r_info), eq(MatchResult::Match));

            // Second log line
            assert_that!(all.matches(&r_info), eq(MatchResult::Complete));
            assert_that!(all_set.matches(&r_info), eq(MatchResult::NotMatch));
            assert_that!(any.matches(&r_info), eq(MatchResult::NotMatch));
            assert_that!(in_order.matches(&r_info), eq(MatchResult::Match));
            assert_that!(set.matches(&r_info), eq(MatchResult::Complete));
            assert_that!(any_set_a.matches(&r_info), eq(MatchResult::Match));
            assert_that!(any_set_b.matches(&r_info), eq(MatchResult::Match));
            assert_that!(group.matches(&r_info), eq(MatchResult::NotMatch));
            assert_that!(any_group.matches(&r_info), eq(MatchResult::NotMatch));

            // Third log line
            assert_that!(all.matches(&r_info), eq(MatchResult::Complete));
            assert_that!(all_set.matches(&r_info), eq(MatchResult::NotMatch));
            assert_that!(any.matches(&r_info), eq(MatchResult::NotMatch));
            assert_that!(in_order.matches(&r_info), eq(MatchResult::NotMatch));
            assert_that!(set.matches(&r_info), eq(MatchResult::NotMatch));
            assert_that!(any_set_a.matches(&r_info), eq(MatchResult::NotMatch));
            assert_that!(any_set_b.matches(&r_info), eq(MatchResult::NotMatch));
            assert_that!(group.matches(&r_info), eq(MatchResult::NotMatch));
            assert_that!(any_group.matches(&r_info), eq(MatchResult::NotMatch));

            // Fourth log line
            assert_that!(all.matches(&r_trace), eq(MatchResult::NotMatch));
            assert_that!(all_set.matches(&r_trace), eq(MatchResult::NotMatch));
            assert_that!(any.matches(&r_trace), eq(MatchResult::Complete));
            assert_that!(in_order.matches(&r_trace), eq(MatchResult::Match));
            assert_that!(set.matches(&r_trace), eq(MatchResult::NotMatch));
            assert_that!(any_set_a.matches(&r_trace), eq(MatchResult::Match));
            assert_that!(any_set_b.matches(&r_trace), eq(MatchResult::Complete));
            assert_that!(group.matches(&r_trace), eq(MatchResult::Match));
            assert_that!(any_group.matches(&r_trace), eq(MatchResult::Complete));

            // Fifth log line
            assert_that!(all.matches(&r_info), eq(MatchResult::Complete));
            assert_that!(all_set.matches(&r_info), eq(MatchResult::NotMatch));
            assert_that!(any.matches(&r_info), eq(MatchResult::NotMatch));
            assert_that!(in_order.matches(&r_info), eq(MatchResult::Complete));
            assert_that!(set.matches(&r_info), eq(MatchResult::NotMatch));
            assert_that!(any_set_a.matches(&r_info), eq(MatchResult::Complete));
            assert_that!(any_set_b.matches(&r_info), eq(MatchResult::NotMatch));
            assert_that!(group.matches(&r_info), eq(MatchResult::Complete));
            assert_that!(any_group.matches(&r_info), eq(MatchResult::NotMatch));

            // Sixth log line
            assert_that!(all.matches(&r_trace), eq(MatchResult::NotMatch));
            assert_that!(all_set.matches(&r_trace), eq(MatchResult::NotMatch));
            assert_that!(any.matches(&r_trace), eq(MatchResult::Complete));
            assert_that!(in_order.matches(&r_trace), eq(MatchResult::NotMatch));
            assert_that!(set.matches(&r_trace), eq(MatchResult::NotMatch));
            assert_that!(any_set_a.matches(&r_trace), eq(MatchResult::NotMatch));
            assert_that!(any_set_b.matches(&r_trace), eq(MatchResult::NotMatch));
            assert_that!(group.matches(&r_trace), eq(MatchResult::NotMatch));
            assert_that!(any_group.matches(&r_trace), eq(MatchResult::NotMatch));

            all.reset();
            all_set.reset();
            any.reset();
            in_order.reset();
            set.reset();
            any_set_a.reset();
            any_set_b.reset();
            group.reset();
            any_group.reset();
        }
    }

    mod macros {
        use googletest::{assert_that, matchers::eq};

        use crate::googletest::debugs_as;

        // Note: we would usually import `super::*` here, but not importing helps us test our macro hygiene.

        #[test]
        fn test_all_macro() {
            assert_that!(all!(), debugs_as(eq("all { expectations: [] }")));
            assert_that!(
                all!(super::contains("abc")),
                debugs_as(eq(
                    "all { expectations: [Contains { substring: \"abc\" }] }"
                ))
            );
            assert_that!(
            all!(super::contains("abc"), super::contains("def")),
            debugs_as(eq("all { expectations: [Contains { substring: \"abc\" }, Contains { substring: \"def\" }] }"))
        );

            assert_that!(
            all!(super::contains("abc"), super::contains("def"), all!(),),
            debugs_as(eq("all { expectations: [Contains { substring: \"abc\" }, Contains { substring: \"def\" }, all { expectations: [] }] }"))
        );
        }

        #[test]
        fn test_any_macro() {
            assert_that!(any!(), debugs_as(eq("any { expectations: [] }")));
            assert_that!(
                any!(super::contains("abc")),
                debugs_as(eq(
                    "any { expectations: [Contains { substring: \"abc\" }] }"
                ))
            );
            assert_that!(
            any!(super::contains("abc"), super::contains("def")),
            debugs_as(eq("any { expectations: [Contains { substring: \"abc\" }, Contains { substring: \"def\" }] }"))
        );

            assert_that!(
            any!(super::contains("abc"), super::contains("def"), any!(),),
            debugs_as(eq("any { expectations: [Contains { substring: \"abc\" }, Contains { substring: \"def\" }, any { expectations: [] }] }"))
        );
        }

        #[test]
        fn test_in_order_macro() {
            assert_that!(
                in_order!(),
                debugs_as(eq("InOrder { expectations: [], previous: None, next: 0 }"))
            );
            assert_that!(
            in_order!(super::contains("abc")),
            debugs_as(eq("InOrder { expectations: [Contains { substring: \"abc\" }], previous: None, next: 0 }"))
        );
            assert_that!(
            in_order!(super::contains("abc"), super::contains("def")),
            debugs_as(eq("InOrder { expectations: [Contains { substring: \"abc\" }, Contains { substring: \"def\" }], previous: None, next: 0 }"))
        );

            assert_that!(
            in_order!(super::contains("abc"), super::contains("def"), in_order!(),),
            debugs_as(eq("InOrder { expectations: [Contains { substring: \"abc\" }, Contains { substring: \"def\" }, InOrder { expectations: [], previous: None, next: 0 }], previous: None, next: 0 }"))
        );
        }

        #[test]
        fn test_set_macro() {
            assert_that!(set!(), debugs_as(eq("Set { unmet_expectations: [], .. }")));
            assert_that!(
                set!(super::contains("abc")),
                debugs_as(eq(
                    "Set { unmet_expectations: [Contains { substring: \"abc\" }], .. }"
                ))
            );
            assert_that!(
            set!(super::contains("abc"), super::contains("def")),
            debugs_as(eq("Set { unmet_expectations: [Contains { substring: \"abc\" }, Contains { substring: \"def\" }], .. }"))
        );

            assert_that!(
            set!(super::contains("abc"), super::contains("def"), set!(),),
            debugs_as(eq("Set { unmet_expectations: [Contains { substring: \"abc\" }, Contains { substring: \"def\" }, Set { unmet_expectations: [], .. }], .. }"))
        );
        }

        #[test]
        fn test_any_set_macro() {
            assert_that!(any_set!(), debugs_as(eq("AnySet { expectations: [], .. }")));
            assert_that!(
                any_set!(super::contains("abc")),
                debugs_as(eq(
                    "AnySet { expectations: [Contains { substring: \"abc\" }], .. }"
                ))
            );
            assert_that!(
            any_set!(super::contains("abc"), super::contains("def")),
            debugs_as(eq("AnySet { expectations: [Contains { substring: \"abc\" }, Contains { substring: \"def\" }], .. }"))
        );

            assert_that!(
            any_set!(super::contains("abc"), super::contains("def"), any_set!(),),
            debugs_as(eq("AnySet { expectations: [Contains { substring: \"abc\" }, Contains { substring: \"def\" }, AnySet { expectations: [], .. }], .. }"))
        );
        }

        #[test]
        fn test_group_macro() {
            assert_that!(
                group!(),
                debugs_as(eq("Group { unmet_expectations: [], .. }"))
            );
            assert_that!(
                group!(super::contains("abc")),
                debugs_as(eq(
                    "Group { unmet_expectations: [Contains { substring: \"abc\" }], .. }"
                ))
            );
            assert_that!(
            group!(super::contains("abc"), super::contains("def")),
            debugs_as(eq("Group { unmet_expectations: [Contains { substring: \"abc\" }, Contains { substring: \"def\" }], .. }"))
        );

            assert_that!(
            group!(super::contains("abc"), super::contains("def"), group!(),),
            debugs_as(eq("Group { unmet_expectations: [Contains { substring: \"abc\" }, Contains { substring: \"def\" }, Group { unmet_expectations: [], .. }], .. }"))
        );
        }

        #[test]
        fn test_predicate_macro() {
            assert_that!(
                predicate!(|_| true),
                debugs_as(eq("Closure { description: \"|_| true\", .. }"))
            );
            assert_that!(
                predicate!(|x| x.line().unwrap() > 0),
                debugs_as(eq(
                    "Closure { description: \"|x| x.line().unwrap() > 0\", .. }"
                ))
            );
        }
    }
}
