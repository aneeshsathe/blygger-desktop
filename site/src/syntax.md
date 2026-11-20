# Writing syntax

Posts are Markdown. On top of it, your blyg understands a few forms of its
own. This page is the reference; [Writing and publishing](writing.md) shows
how to use them.

| You write | You get | Where |
|---|---|---|
| `[[id]]`, `[[id \| label]]` | a link to a post | anywhere |
| `![[id]]`, `![[id \| note]]` on its own line | a quote of a post | threads |
| `![[id]]` then `>` lines | a quote of just that passage | threads |
| `[[+ idea]]` | a new item, linked | threads |
| `![[+ passage]]` | a new item, quoted | threads |
| `$x^2$`, `$$…$$` | a formula | anywhere |
| `/* … */` | a note to yourself, never published | anywhere |
| `[TK]instruction[/TK]` | a gap for later, or for AI | anywhere |

Links, quotes and labels are under
[Quoting other posts](writing.md#quoting-other-posts), and TK under
[TK: gaps for later](writing.md#tk-gaps-for-later).

## Mathematics

Write LaTeX between dollar signs. One dollar on each side makes a formula
inside a sentence; two make one on its own lines:

```markdown
The area of a circle is $A = \pi r^2$, and Basel's sum is

$$\sum_{n=1}^{\infty} \frac{1}{n^2} = \frac{\pi^2}{6}$$
```

Money stays money. A dollar followed by a space doesn't open a formula, and
a dollar followed by a digit doesn't close one, so `$5 and $10` is just text.
To write a dollar where it would start a formula, put a backslash before it:
`\$`. Dollars in code are code.

**In the preview,** each formula is drawn as readers will see it. One that
doesn't parse shows in red; rest the pointer on it for the reason. Your blyg
won't publish a post with a broken formula, so fix it first.

**Keys:** select some text and press ⌃⌘M (Alt+M on Windows, or
**Edit › Format › Formula**) to put it between dollars. A selection over more
than one line becomes `$$…$$`.

**Readers** see formulas drawn from their TeX, in Burrow's reader and on
your blyg's pages. In the Stream's cards a formula shows as its TeX.

**Quoting a passage:** a passage quote can't cut through a formula. Quote the
whole post, or the prose either side of it.

Formulas need blygger-studio 0.62 or later. Very long formulas, or a great
many in one post, are shown as written instead of drawn, as your blyg does.

## New items: `![[+ …]]` and `[[+ …]]`

In a thread, these turn part of what you're writing into posts of their own
when you publish (blygger-studio 0.63 or later). This is called
**tangling**.

```markdown
Notes from the harbour walk.

![[+ The tide tables were wrong by an hour.

Nobody on the quay seemed to mind.]]

Next time I'll write up [[+ the ferry timetable]] properly.
```

**`![[+ passage]]`** becomes a new item, and the thread quotes it in its
place.

- `![[+` must start a line, followed by a space or a line break.
- `]]` must end a line.
- In between, the passage can run over several lines and paragraphs, and may
  hold `[[id]]` links and `![[id]]` quotes.

**`[[+ idea]]`**, inside a sentence, becomes a new item whose text is the
idea, as its heading. The thread links to it.

**Rules:**

- Threads only. In a fragment, publishing is refused with "tangling makes a
  thread; switch this draft to a thread".
- At most 10 new items per publish ("one publish can make at most 10 new
  items; this has 11").
- A passage can't hold another `![[+ …]]`, a `[[+ …]]`, a TK gap or an
  attached image.
- They're plain text in code, inside TK gaps and inside comments to self.

**When you publish,** Burrow lists the new items and asks first. ⏎ publishes
them, then the thread; esc goes back to your text. Your blyg then rewrites
each one as a reference to the item it made, `![[id | first words…]]` or
`[[id | text]]`, and that's what the editor shows from then on. Each new item
is a thread of its own: to change one, open it.

## Comments to self

`/* … */` is a note to yourself. It stays in your draft, the preview leaves it
out, and your blyg removes it when you publish, so it never appears on the
page:

```markdown
The ferry leaves at nine. /* check the winter timetable */
```

- A comment starts at the beginning of a line or after a space. `/**`,
  `/*` touching a word, and `\/*` aren't comments.
- It ends at the first `*/`. It can run over several lines.
- In code it's code.

If a comment would stop the post publishing (one that never closes, a stray
`*/`, or a `/*` stuck to a word), the preview says so in ⚠ lines at its end.

## Quotes, labels and attribution

A quote can carry a label: `![[id | label]]`. On the published page, the
quote's attribution line leads with the label, then says where it's from:
*label ↗ · from Ada's blyg · snapshot of v5*.

When a post you quote itself quotes another, the inner quote gets its own,
smaller attribution line, however deep the quotes go.

In a passage quote, a line straight under a `>` line, with no `>` of its own
and no blank line between, still belongs to the passage:

```markdown
![[01J9ZK3Q7T5V8X2B4N6M0PQRST]]
> The passage you want,
exactly as it is in the post.
```

Leave a blank line after the passage before your own words.
