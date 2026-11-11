# Syntax highlighting demo

A scratch document for checking MD Viewer's fenced-code highlighting. Nothing here
is special Markdown — it is just one block per language the highlighter knows, plus
a few traps.

What to look for: comments italic grey, strings green, numbers orange, keywords
red; HTML/XML element names and attributes take the tag/attribute colours. Plain
identifiers and punctuation keep the normal code colour — that is deliberate, and
it is what keeps the rendered payload small.

## Rust

```rust
// A line comment runs to the end of the line.
fn main() {
    let count: usize = 42;              // fn + let are keywords, 42 is a number
    let text = "a string with \"escapes\" and & < > in it";
    /* a block comment,
       across two lines */
    println!("{count} {text}");
}
```

## JavaScript / TypeScript

```js
// greeting
const who = "world";
let n = 42;
console.log(`hello ${who}`, n);          // a template literal is one string
class Thing { constructor(x) { this.x = x; } }
```

## Python

```python
# a comment
def render(name):
    """A docstring, which is a string
    that spans lines."""
    return f"hello {name}", 3.5
```

## JSON

```json
{
  "name": "md-viewer",
  "version": "0.2.0",
  "stable": true,
  "count": 12
}
```

## Shell

```bash
# install and build
cd src-tauri && cargo build --release
if [ -f sample.md ]; then
  echo "found it" | tee log.txt
fi
```

## PowerShell

```powershell
# build the viewer
<# a block comment #>
function Get-Document { param($path) return "$path" }
```

## SQL

```sql
-- find the documents
SELECT id, name FROM documents WHERE name LIKE '%note%' ORDER BY id DESC;
```

## YAML and TOML

```yaml
# configuration
name: md-viewer
stable: true
paths: ["a.md", "b.md"]
```

```toml
# configuration
[viewer]
name = "md-viewer"
scale = 1.0
```

## CSS

```css
/* the outline drawer */
.toc-list a {
  color: #20242a;
  padding: 0.4rem 0.6rem;
}
```

## HTML / XML

```html
<!-- a marker -->
<a href="/notes">&amp; link</a>
<picture><source srcset="icon.webp"><img alt="icon" src="icon.png"></picture>
```

## C family

```c
#include <stdio.h>
int main(void) {
    // hello
    char c = 'x';
    printf("n = %d\n", 7);
    return 0;
}
```

## Traps

Things that must **not** be highlighted as if they were markup or code structure.

```js
// Markup inside a string stays text.
const html = "<img src=x onerror=alert(1)>";
// An apostrophe must not swallow the rest of the line.
const quoted = "it's fine";
```

## Languages it does not know

```cobol
       DISPLAY 'HI'.
```

```
A fence with no language tag at all. It should look exactly as it did before this
feature existed: no token colours, and no extra spans in the payload.
```

## Inline code

Inline `cargo build --release` is untouched — only fenced blocks are tokenised.
