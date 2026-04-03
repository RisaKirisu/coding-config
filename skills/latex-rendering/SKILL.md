---
name: latex-rendering
description: Latex rendering instructions and examples. Load this skill before you generate any latex text to the user. 
license: MIT
compatibility: opencode
metadata:
  audience: all
---
When outputting Latex, adhere to the following instruction:
Use these KaTeX delimiters:

- Inline math: `$...$`
- Display math: `$$...$$`
- Do not use: `\(...\)` or `\[...\]`

Guidelines:
- Use `$...$` inside normal sentences.
- Use `$$...$$` for standalone equations.
- `$$...$$` works both on one line and across multiple lines.
- `\(...\)` and `\[...\]` are not rendered here.

Examples

Inline:
```text
The roots are $x=1$ and $x=2$.
```

```text
Energy is $E=mc^2$.
```

Display:
```text
$$x^2+y^2=z^2$$
```

```text
$$
\int_0^1 x^2\,dx=\frac{1}{3}
$$
```

Unsupported:
```text
\(x^2+y^2=z^2\)
```

```text
\[
x^2+y^2=z^2
\]
```

Safe rule:
- For inline math, always use `$...$`.
- For block math, always use `$$...$$`.