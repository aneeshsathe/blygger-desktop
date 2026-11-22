//! --- dictionary --- The owner-API calls for the blyg's dictionary (studio
//! 0.69, contract 6.2.0; shapes and rules in `crate::dictionary`). Asked
//! only of a studio whose capabilities list `getDictionary` and
//! `putDictionarySource`.

use super::{Api, enc};
use crate::backend::{CoreError, Result};
use crate::dictionary::{Dictionary, parse, put_body};

/// What a `PUT /api/dictionary/{source}` came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PutDictionary {
    /// Stored: the dictionary as the blyg keeps it now.
    Stored(Dictionary),
    /// 409 `revision-conflict`: it moved since it was read. Nothing was
    /// stored; the dictionary as it is now, read again (`None` when that
    /// read failed).
    Conflict(Option<Dictionary>),
}

impl Api {
    /// Whether this studio offers the dictionary (both operations).
    pub fn serves_dictionary(&self) -> Result<bool> {
        Ok(self.offers_op(crate::dictionary::GET_OPERATION)?
            && self.offers_op(crate::dictionary::PUT_OPERATION)?)
    }

    /// `GET /api/dictionary`. `Ok(None)`: not offered here.
    pub fn dictionary(&self) -> Result<Option<Dictionary>> {
        if !self.serves_dictionary()? {
            return Ok(None);
        }
        match Self::optional(self.call("GET", "/api/dictionary", None))? {
            Some(v) => parse(&v).map(Some).ok_or_else(unexpected),
            None => Ok(None),
        }
    }

    /// `PUT /api/dictionary/{source} {words, expectedRevision}`: replace one
    /// source's whole list.
    pub fn put_dictionary_source(
        &self,
        source: &str,
        words: &[String],
        expected_revision: u64,
    ) -> Result<PutDictionary> {
        let path = format!("/api/dictionary/{}", enc(source));
        match self.call("PUT", &path, Some(put_body(words, expected_revision))) {
            Ok(v) => parse(&v).map(PutDictionary::Stored).ok_or_else(unexpected),
            Err(CoreError::RevisionConflict) => {
                Ok(PutDictionary::Conflict(self.dictionary().ok().flatten()))
            }
            Err(e) => Err(e),
        }
    }
}

fn unexpected() -> CoreError {
    CoreError::Other("unexpected response from /api/dictionary".into())
}
