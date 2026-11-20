//! Pasting into the editor: a web address over selected text becomes a
//! link, and images (pasted or dropped) upload to the blyg's media.

use super::*;

impl MainView {
    /// A web address pasted over selected text links the text: `[text](url)`.
    pub(super) fn paste_link(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.current.is_none() {
            return false;
        }
        let Some(clip) = cx.read_from_clipboard().and_then(|c| c.text()) else {
            return false;
        };
        let (text, sel) = {
            let s = self.editor.read(cx);
            (s.value().to_string(), s.selected_range())
        };
        let Some((new, caret)) = vm::link_paste(&text, sel, &clip) else {
            return false;
        };
        self.splice_editor(&text, &new, Some(caret), window, cx);
        true
    }

    pub(super) fn paste_image(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(item) = cx.read_from_clipboard() else {
            return false;
        };
        let image = item.entries().iter().find_map(|e| match e {
            ClipboardEntry::Image(img) => Some(img.clone()),
            _ => None,
        });
        if let Some(img) = image {
            let mime = img.format.mime_type();
            self.start_upload(img.bytes, mime.to_string(), window, cx);
            return true;
        }
        // Copied image files arrive as paths.
        let paths: Vec<std::path::PathBuf> = item
            .entries()
            .iter()
            .filter_map(|e| match e {
                ClipboardEntry::ExternalPaths(p) => Some(p.paths().to_vec()),
                _ => None,
            })
            .flatten()
            .filter(|p| vm::mime_for_path(p).is_some())
            .collect();
        if paths.is_empty() {
            return false;
        }
        self.upload_paths(paths, window, cx);
        true
    }

    pub(super) fn upload_paths(
        &mut self,
        paths: Vec<std::path::PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for p in paths {
            let Some(mime) = vm::mime_for_path(&p) else {
                continue;
            };
            match std::fs::read(&p) {
                Ok(bytes) => self.start_upload(bytes, mime.to_string(), window, cx),
                Err(e) => self.show_toast(format!("Couldn't read {}: {e}", p.display()), None, cx),
            }
        }
    }

    pub(super) fn start_upload(
        &mut self,
        bytes: Vec<u8>,
        mime: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(id) = self.current.as_ref().map(|c| c.local_id.clone()) else {
            self.show_toast("Open or create a post first", None, cx);
            return;
        };
        // --- follow-ups --- a scratch note keeps its images on this Mac.
        if self.scratch_keep_image(&bytes, &mime, window, cx) {
            return;
        }
        // Insert the placeholder at the caret, as an undoable edit.
        let (text, cursor) = {
            let s = self.editor.read(cx);
            (s.value().to_string(), s.cursor())
        };
        let (new_text, range) = vm::insert_placeholder(&text, cursor);
        self.splice_editor(&text, &new_text, Some(range.end), window, cx);
        let size_kb = bytes.len().div_ceil(1024);
        let backend = self.backend.clone();
        // Uploaded *unattached*: the Worker appends every attachment to the
        // public page, so an attached image that's also inline would show
        // twice. Inline, it shows once, where it was pasted.
        let task = cx.background_spawn(async move {
            let r = backend.upload_media(bytes.clone(), &mime, None, None);
            // The text gets the absolute URL: the Worker renders a relative
            // `media/…` page-relative, which breaks under /f/<id>/.
            r.map(|m| {
                let inline = crate::vm::absolute_url(&m.url, backend.base_url().as_deref())
                    .unwrap_or_else(|| m.url.clone());
                (m, inline)
            })
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |v, window, cx| {
                let editing_same = v.current.as_ref().is_some_and(|c| c.local_id == id);
                let text = if editing_same {
                    v.editor.read(cx).value().to_string()
                } else {
                    v.backend
                        .item(&id)
                        .map(|i| i.content_md)
                        .unwrap_or_default()
                };
                let replaced = match &result {
                    Ok((_, inline)) => vm::replace_placeholder(&text, inline),
                    Err(_) => vm::remove_placeholder(&text),
                };
                // --- upkeep --- once the image is in place, ask for its alt text.
                let alt_for = result
                    .as_ref()
                    .ok()
                    .filter(|_| replaced.is_some() && editing_same)
                    .map(|(_, inline)| inline.clone());
                // The placeholder was deleted before the upload finished:
                // nothing refers to the file, so remove it again.
                if replaced.is_none()
                    && let Ok((m, _)) = &result
                {
                    let (b, url) = (v.backend.clone(), m.url.clone());
                    cx.background_spawn(async move {
                        let _ = b.delete_media(&url);
                    })
                    .detach();
                }
                if let Some(new_text) = replaced {
                    if editing_same {
                        v.splice_editor(&text, &new_text, None, window, cx);
                    } else if v.restoring.as_ref() != Some(&id) {
                        let _ = v.backend.save(&id, &new_text);
                    }
                }
                match result {
                    Ok((m, _)) => {
                        let ext = m.url.rsplit('.').next().unwrap_or("");
                        v.show_toast(format!("Image uploaded ({size_kb} KB {ext})"), None, cx)
                    }
                    Err(e) => v.show_toast(format!("Upload failed: {e}"), None, cx),
                }
                if let Some(url) = alt_for {
                    v.upkeep_ask_alt(id.clone(), url, window, cx); // --- upkeep ---
                }
            });
        })
        .detach();
    }
}
