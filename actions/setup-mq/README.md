# Setup mq

This GitHub Action will setup mq in your GitHub Actions workflow, allowing you
to easily integrate mq into your CI/CD pipeline.

## Usage

```yaml
steps:
  - uses: actions/checkout@v4
  - name: Setup mq
    uses: mq/actions/setup-mq@v1
    with:
      version: 'v0.1.0' # Optional: defaults to latest
```

## Inputs

| Name      | Description           | Required | Default        |
| --------- | --------------------- | -------- | -------------- |
| `version` | mq version to install | No       | Latest version |

## License

This GitHub Action is available under the [MIT License](LICENSE).
