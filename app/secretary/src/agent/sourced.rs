//! The message a todo came from, as a call spells it: four arguments, all or none.

use prodrome_core::fpl;

use suzatary_base::escape::escape_string;
use suzatary_base::refusal::Refusal;
use suzatary_inbox::priced::day_of;
use suzatary_inbox::verb::Caught;
use suzatary_record::record::Source;

/// The `source_*` arguments of a call.
#[derive(Debug, Default)]
pub(crate) struct Sourced {
    pub sender: Caught<Option<String>>,
    pub subject: Caught<Option<String>>,
    pub date: Caught<Option<String>>,
    pub hash: Caught<Option<String>>,
}

impl Sourced {
    /// The source, when all four are given.
    ///
    /// # Errors
    /// A part of the wrong type, some but not all four given, or a date that is not a day.
    pub(crate) fn source(&self) -> Result<Option<Source>, Refusal> {
        let given = (
            self.sender.read()?,
            self.subject.read()?,
            self.date.read()?,
            self.hash.read()?,
        );
        let (Some(sender), Some(subject), Some(date), Some(hash)) = given else {
            if let (None, None, None, None) = given {
                return Ok(None);
            }
            return Err(Refusal::bad_request(
                "source is all four fields or none: source_sender, source_subject, \
                 source_date (YYYY-MM-DD) and source_hash",
            ));
        };
        let day = day_of(date, "source_date")?;
        let at = fpl::datetime_of(chrono::NaiveDateTime::from(day))
            .map_err(|e| Refusal::bad_request(e.to_string()))?;
        Ok(Some(suzatary_record::record::mk_source(
            escape_string(sender).as_str(),
            escape_string(subject).as_str(),
            at,
            escape_string(hash).as_str(),
            "",
            "",
        )))
    }
}
