# mq-ruby

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

Ruby bindings for [mq](https://mqlang.org/), a jq-like command-line tool for processing Markdown.

## Ruby API

Once complete, the Ruby API will look like this:

```ruby
require 'mq'

# Basic usage
markdown = <<~MD
  # Main Title
  ## Section 1
  Some content here.
  ## Section 2
  More content.
MD

result = MQ.run('.h2', markdown)
result.values.each do |heading|
  puts heading
end
# => ## Section 1
# => ## Section 2

# With options
options = MQ::Options.new
options.input_format = MQ::InputFormat::HTML

result = MQ.run('.h1', '<h1>Hello</h1><p>World</p>', options)
puts result.text  # => # Hello

# HTML to Markdown conversion
html = '<h1>Title</h1><p>Paragraph</p>'
markdown = MQ.html_to_markdown(html)
puts markdown  # => # Title\n\nParagraph
```

## License

The gem is available as open source under the terms of the [MIT License](https://opensource.org/licenses/MIT).

## Links

- [mq Website](https://mqlang.org/)
- [GitHub Repository](https://github.com/harehare/mq)
- [Command-line Tool](https://github.com/harehare/mq#installation)
