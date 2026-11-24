//! The picker-sources test extension (`tests/picker.rs`, and blyg-app's
//! picker tab tests run the same answers from their own child):
//! answers `extension/picker.search` from the query.
//!
//! - `broken`: an error;
//! - `many`: 60 results (more than a tab shows);
//! - anything else: a labelled result whose excerpt spans lines and whose
//!   meta repeats what was asked, a bare one, one that isn't an item id,
//!   and a repeat.

#![forbid(unsafe_code)]

use blyg_ext::ext::{Extension, HostClient, serve};
use blyg_ext::picker::{PICKER_SEARCH, PickerSearchParams};
use blyg_ext::protocol::*;
use blyg_ext::rpc::params;
use serde_json::Value;

struct Picker;

impl Extension for Picker {
    fn initialize(
        &mut self,
        _host: &HostClient,
        _p: InitializeParams,
    ) -> Result<InitializeResult, RpcError> {
        Ok(InitializeResult {
            protocol_version: PROTOCOL_VERSION,
            name: "bxp-picker".into(),
            version: "0.0.1".into(),
            commands: vec![],
            sources: vec![],
            libraries: vec![],
        })
    }

    fn request(&mut self, _host: &HostClient, method: &str, p: Value) -> Result<Value, RpcError> {
        if method != PICKER_SEARCH {
            return Err(RpcError::method_not_found(method));
        }
        let p: PickerSearchParams = params(p)?;
        blyg_ext::picker::sample_answer(&p).map_err(|e| RpcError::new(-1, e))
    }
}

fn main() {
    serve(std::io::stdin(), std::io::stdout(), &mut Picker);
}
