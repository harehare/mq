# Decode a CBOR payload

Goal: Inspect a [CBOR](https://cbor.io) payload, either a binary file or a base64 string copied from a log or a token, as JSON. The reverse direction, encoding JSON as CBOR, is covered too.

Prerequisites: The built-in `cbor` module. Read a binary file with `-I cbor`. For a base64 string, read it with `-I raw` and call `cbor_parse`, which accepts base64 text as well as raw bytes.

## Query

A binary file:

```bash
$ mq -I cbor -F json '.' payload.cbor
```

A base64 string:

```bash
$ echo 'o2JpZAdkdGFnc4JhYWFiYm9r9Q==' | mq -I raw -F json 'import "cbor" | cbor::cbor_parse'
```

## Input

`payload.cbor` holds these 19 bytes (shown as hex), and the base64 string above is the same data:

```
a3 62 69 64 07 64 74 61 67 73 82 61 61 61 62 62 6f 6b f5
```

## Output

Both commands print:

```json
{
  "id": 7,
  "tags": [
    "a",
    "b"
  ],
  "ok": true
}
```

## Pick out one field

```bash
$ mq -I cbor 'get("tags")' payload.cbor
```

```
["a", "b"]
```

## Encode JSON as base64 CBOR

```bash
$ echo '{"id": 7, "tags": ["a", "b"], "ok": true}' | mq -I json 'import "cbor" | cbor::cbor_stringify | base64'
```

```
o2JpZPlHAGR0YWdzgmFhYWJib2v1
```

## Notes

- `cbor_stringify` returns bytes. Printed directly they appear as hex (`a3626964f94700…`), and `-o` writes that hex text rather than a binary file. Pipe through `base64` when you need something you can paste elsewhere.
- The base64 above differs from the decode example even though both describe the same data. `cbor_stringify` writes numbers as CBOR floats, so `7` becomes `f94700` instead of the one-byte integer `07`. Decoding either form gives `7`.
- A truncated or invalid payload stops with a `Failed to parse CBOR` error instead of returning partial data.
- Once decoded, the data is an ordinary mq value, so `-F` can write it in any output format, for example `-F yaml` or `-F toml`.
