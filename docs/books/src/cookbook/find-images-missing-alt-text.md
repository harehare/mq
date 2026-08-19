# Find images missing alt text

Goal: Spot images with no alt text, a quick accessibility check before publishing.

Prerequisites: None.

## Query

```mq
select(.image.alt == "")
```

```bash
$ mq 'select(.image.alt == "")' README.md
```

Just the file paths, for a punch list:

```bash
$ mq 'select(.image.alt == "") | .image.url' README.md
```

## Input

```markdown
![A cute cat](cat.png)

![](missing-alt.png)

![Team photo](team.jpg)
```

## Output

```markdown
![](missing-alt.png)
```

## Notes

- `.image.alt` on its own skips images with empty alt text (selectors drop falsy matches), so it's only useful for listing the alt text that *does* exist. To find the gaps, filter explicitly with `select(.image.alt == "")`.
- Gotcha: a non-matching node's result is `None` after `select`, and `None` results are what actually get suppressed from output, not "filtering" in a control-flow sense. Plain field access on that `None` (like `.image.url` above) stays `None` and stays suppressed. But routing it through something that produces a real value even for `None` input, such as string concatenation (`__FILE__ + ": " + .image.url`) or a function like `is_none(self)`, "launders" it into a non-`None` result, and it prints for *every* node, defeating the filter. Keep transformations after `select` limited to plain field/selector access, or restructure so the `select` is the last step.

