# How to tangle

**Tangling** turns part of a thread into a post of its own, as you publish.
You write the thread the way you think it, mark the passages and ideas that
deserve their own post, and one publish makes them all. The thread then
quotes or links each new post in place.

It needs blygger-studio 0.63 or later on your blyg. On an older studio, the
marks are plain text.

## Before you start

- **The draft must be a thread.** Fragments can't quote, so tangling can't
  work in one. Press **⌘T** to switch a draft to a thread.
- **Up to 10 new posts per publish.**

## 1. Mark a passage: `![[+ … ]]`

Put `![[+ ` at the start of a line and `]]` at the end of a line. Everything
in between becomes a new post, and the thread quotes it where it stood:

```markdown
Notes from the harbour walk.

![[+ The tide tables were wrong by an hour. Nobody on the quay
seemed to mind; the boats left when the water said so.]]
```

The passage can run over several paragraphs, and it can hold `[[id]]` links
and `![[id]]` quotes of other posts.

## 2. Mark an idea: `[[+ … ]]`

Inside a sentence, `[[+ an idea]]` becomes a short new post with the idea as
its heading, and the thread links to it. This is how you leave yourself a
post to write later:

```markdown
Next time I'll write up [[+ the ferry timetable]] properly.
```

## 3. Check the preview

The preview shows each passage as a dashed quote marked **New item**, and each
idea as a dashed link with the same tag. Nothing has been published yet.

![A draft with a passage and an idea marked for tangling, and its preview](screenshots/tangle-preview.png)

## 4. Publish

Publish as usual: **⌘⏎**, then **⏎**. Before anything goes out, Burrow lists
what it will make: "This will publish 2 new items and this thread". An idea's
line ends "(linked)".

![The confirm step listing the two new items](screenshots/tangle-confirm.png)

- **⏎** publishes them all: the new posts first, then the thread.
- **esc** goes back to your text, and nothing is published.

The toast then says "Published … · and 2 new items".

## 5. What your draft looks like afterwards

Your blyg replaces each mark with a reference to the post it made:

```markdown
![[7c9wk2n4h6q1x8v0z3m5rjy2ke | The tide tables were wrong…]]

Next time I'll write up [[0b4mf8q2xr6k1n9v3z7h5jc2wd | the ferry timetable]] properly.
```

From then on, that's what the editor shows. **⌘Z stops there**, so undo can't
bring the `![[+ …]]` back and make the posts a second time. If the same marked
text reaches the confirm step again, Burrow warns you first.

## Changing a new post later

Each new post is a thread of its own. To change it, open it from your list (or
click the quote) and edit it like any post. The parent thread quotes the
version it was published with. Burrow's lineage marks show when a newer
version exists, the same as for any quote.

## Going deeper: tangling inside a part

You can't mark a passage inside another passage. Once a new post exists,
though, you can open it and tangle again from there. Each publish adds one
level, so a thought can branch into a tree of posts over time.

## What your readers see

The parts arrive together with the thread, so they don't flood anyone's
reading list. Burrow puts them in **Background** with a badge,
"published with 'Notes from the harbour walk'". The thread shows **+2 parts**:

![A thread in the Stream with "+2 parts · p unfolds"](screenshots/tangle-parts.png)

- **p** unfolds the parts under the thread, and **1**–**9** opens one.
- Readers who want parts in their main lists turn off **Parts published with
  a thread** in Settings › Background.

## When Burrow says no

These are caught before anything is sent, in the studio's own words:

| Message | Fix |
|---|---|
| tangling makes a thread; switch this draft to a thread | Press ⌘T. |
| a new-item span must start its own line | Move `![[+` to the start of a line. (For an idea mid-sentence, use `[[+ … ]]`.) |
| write a space after ![[+ | `![[+ text]]`, not `![[+text]]`. |
| a new-item span's closing ]] must end its line | Put nothing after the `]]` on that line. |
| this new-item span has no closing ]] | Add the `]]`. |
| this new-item span is empty / this [[+ …]] placeholder is empty | Write something inside, or remove the mark. |
| a new-item span cannot contain another | Tangle again inside the new post once it exists. |
| a [[+ …]] placeholder cannot sit inside a ![[+ …]] span | Add it to the new post once it exists. |
| a new-item span cannot contain a TK scope | Generate the TK first, or move it out of the passage. |
| a new-item span cannot contain an attached image yet | Attach the image to the new post after it's published. |
| one publish can make at most 10 new items | Split the publish, or turn some marks into ordinary text. |

`![[+` and `[[+` are plain text inside code, inside TK gaps and inside
comments to self. Typing them doesn't open the link picker.

## Not the same as a series

Tangling makes a hub with spokes: one thread and the posts it quotes. A series
where each post points to the next is still made by hand: publish the next
post and link it with `[[id]]`.

See [Writing syntax](syntax.md) for the exact grammar, and
[Reading](reading.md) for Background and parts.
