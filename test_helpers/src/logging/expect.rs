use std::{collections::HashSet, fmt::Debug};

/// Indicates the result of a matching attempt.
pub enum MatchResult {
    /// Indicates the `Expectation` couldn't match to the `log::Record`
    NotMatch,
    /// Indicates that the `Expectation` did match to the `log::Record`, but is not yet completely matched.
    /// In practice, this often means the expectation is trying to match against multiple log messages and is still
    /// waiting for more.
    Match,
    /// Indicates that the `Expectation` did match to the `log::Record` and is completely fulfilled.
    Complete,
}

pub trait Expectation: Send + 'static + Debug {
    fn matches(&mut self, record: &log::Record) -> MatchResult;
    fn reset(&mut self);
}

/// Matches logs against a specific log level, module path and message. Must match exactly.
pub fn exact(
    level: log::Level,
    module_path: impl Into<String>,
    message: impl Into<String>,
) -> impl Expectation {
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
pub fn contains(substring: impl Into<String>) -> impl Expectation {
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
        predicate($x, stringify!($x))
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
    [ $( $x:expr ),* ] => {
        {
            let mut b_vec = Vec::new();
            $(
                b_vec.push(Box::new($x));
            )*
            all(b_vec)
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
    [ $( $x:expr ),* ] => {
        {
            let mut b_vec = Vec::new();
            $(
                b_vec.push(Box::new($x));
            )*
            any(b_vec)
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
#[macro_export]
macro_rules! in_order {
    [ $( $x:expr ),* ] => {
        {
            let mut b_vec = Vec::new();
            $(
                b_vec.push(Box::new($x));
            )*
            in_order(b_vec)
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
    [ $( $x:expr ),* ] => {
        {
            let mut b_vec = Vec::new();
            $(
                b_vec.push(Box::new($x));
            )*
            set(b_vec)
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
    [ $( $x:expr ),* ] => {
        {
            let mut b_vec = Vec::new();
            $(
                b_vec.push(Box::new($x));
            )*
            any_set(b_vec)
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
    [ $( $x:expr ),* ] => {
        {
            let mut b_vec = Vec::new();
            $(
                b_vec.push(Box::new($x));
            )*
            group(b_vec)
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
            .finish()
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
        f.debug_struct("all")
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
                &self.expectations.iter().enumerate().filter_map(|(i, e)| {
                    if self.complete.contains(&i) {
                        None
                    } else {
                        Some(e)
                    }
                }),
            )
            .finish()
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
        f.debug_struct("Set")
            .field("expectations", &self.expectations)
            .finish()
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
        f.debug_struct("Set")
            .field(
                "unmet_expectations",
                &self.expectations.iter().enumerate().filter_map(|(i, e)| {
                    if self.complete.contains(&i) {
                        None
                    } else {
                        Some(e)
                    }
                }),
            )
            .finish()
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
