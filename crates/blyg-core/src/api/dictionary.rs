//! --- dictionary --- The owner-API calls for the blyg's dictionary (studio
//! 0.69, contract 6.2.0; shapes and rules in `crate::dictionary`). Asked
//! only of a studio whose capabilities list `getDictionary` (a read) or
//! `putDictionarySource` (a write).

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
    /// blyg no longer offers it).
    Conflict(Option<Dictionary>),
}

impl Api {
    /// Whether this studio offers the dictionary to read.
    pub fn serves_dictionary(&self) -> Result<bool> {
        self.offers_all(&[crate::dictionary::GET_OPERATION])
    }

    /// Whether it offers changing a source's words.
    pub fn serves_dictionary_write(&self) -> Result<bool> {
        self.offers_all(&[crate::dictionary::PUT_OPERATION])
    }

    /// `GET /api/dictionary`. `Ok(None)`: not offered here. Its `writable`
    /// says whether this sign-in may change it (the capabilities, cached).
    pub fn dictionary(&self) -> Result<Option<Dictionary>> {
        if !self.serves_dictionary()? {
            return Ok(None);
        }
        let Some(v) = Self::optional(self.call("GET", "/api/dictionary", None))? else {
            return Ok(None);
        };
        let mut d = parse(&v).ok_or_else(unexpected)?;
        d.writable = self.serves_dictionary_write()?;
        Ok(Some(d))
    }

    /// `PUT /api/dictionary/{source} {words, expectedRevision}`: replace one
    /// source's whole list. A re-read after a conflict that fails is the
    /// error (a network failure keeps a queued edit).
    pub fn put_dictionary_source(
        &self,
        source: &str,
        words: &[String],
        expected_revision: u64,
    ) -> Result<PutDictionary> {
        if !self.serves_dictionary_write()? {
            return Err(CoreError::Other(
                "your blyg doesn't let this sign-in change its dictionary".into(),
            ));
        }
        let path = format!("/api/dictionary/{}", enc(source));
        match self.call("PUT", &path, Some(put_body(words, expected_revision))) {
            Ok(v) => parse(&v).map(PutDictionary::Stored).ok_or_else(unexpected),
            Err(CoreError::RevisionConflict) => Ok(PutDictionary::Conflict(self.dictionary()?)),
            Err(e) => Err(e),
        }
    }
}

fn unexpected() -> CoreError {
    CoreError::Other("unexpected response from /api/dictionary".into())
}
