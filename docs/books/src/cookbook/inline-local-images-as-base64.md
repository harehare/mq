# Inline local images as base64

Goal: Make a document self-contained by embedding its local image files directly into it as `data:` URIs, handy for pasting a doc somewhere that can't resolve relative image paths (a Slack message, a single-file export).

Prerequisites: The `--allow-read` flag, since this reads image files from disk. Only local images are touched; URLs with a scheme (`https://`, ...) or already-`data:` URIs are left alone.

## Query

```bash
$ mq --allow-read=. 'select(.image) | embed_images(., ".")' doc.md
```

## Input

```markdown
![Logo](logo.png)

![External](https://example.com/pic.png)
```

## Output

```markdown
![Logo](data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGP4z8AAAAMBAQAY3Y2wAAAAAElFTkSuQmCC)

![External](https://example.com/pic.png)
```

## Notes

- Unlike most mq functions, `embed_images` takes the node as an **explicit** first argument, not implicitly through the pipe: `embed_images(., base_dir)`, not `embed_images(base_dir)` alone. Calling it as `embed_images(base_dir)` silently returns `base_dir` itself instead of erroring, which is easy to miss.
- The base directory (second argument, default `"."`) is where relative image paths like `logo.png` are resolved from. It's usually the directory containing the Markdown file, not the current working directory, if they differ.
- Combine with `-U` to keep the rest of the document (non-image nodes) intact in the output.
