# frozen_string_literal: true

require "spec_helper"

RSpec.describe MQ do
  describe ".run" do
    context "with basic markdown queries" do
      it "extracts h1 headings" do
        content = "# Hello World\n\n## Heading2\n\nText"
        result = MQ.run(".h1", content, nil)
        expect(result.values).to eq(["# Hello World"])
      end

      it "extracts h2 headings" do
        content = "# Hello World\n\n## Heading2\n\nText"
        result = MQ.run(".h2", content, nil)
        expect(result.values).to eq(["## Heading2"])
      end

      it "extracts multiple h2 headings" do
        content = "# Main Title\n\n## Heading2A\n\nText\n\n## Heading2B\n\nMore text"
        result = MQ.run(".h2", content, nil)
        expect(result.values).to eq(["## Heading2A", "## Heading2B"])
      end

      it "filters headings with select" do
        content = "# Product\n\n## Features\n\nText\n\n## Installation\n\nMore text"
        result = MQ.run('.h2 | select(contains("Feature"))', content, nil)
        expect(result.values).to eq(["## Features"])
      end

      it "extracts list items" do
        content = "# List\n\n- Item 1\n- Item 2\n- Item 3"
        result = MQ.run(".[]", content, nil)
        expect(result.values).to eq(["- Item 1", "- Item 2", "- Item 3"])
      end

      it "extracts code blocks" do
        content = "# Code\n\n```python\nprint('Hello')\n```"
        result = MQ.run(".code", content, nil)
        expect(result.values).to eq(["```python\nprint('Hello')\n```"])
      end
    end

    context "with different input formats" do
      it "processes TEXT format" do
        options = MQ::Options.new
        options.input_format = MQ::InputFormat::TEXT

        content = "Line 1\nLine 2\nLine 3"
        result = MQ.run('select(contains("2"))', content, options)
        expect(result.values).to eq(["Line 2"])
      end

      it "processes MDX format" do
        options = MQ::Options.new
        options.input_format = MQ::InputFormat::MDX

        content = "# MDX Content\n\n<Component />"
        result = MQ.run("select(is_mdx())", content, options)
        expect(result.values).to eq(["<Component />"])
      end

      it "processes HTML format" do
        options = MQ::Options.new
        options.input_format = MQ::InputFormat::HTML

        content = "<h1>Hello</h1><p>World</p>"
        result = MQ.run('select(contains("Hello"))', content, options)
        expect(result.values).to eq(["# Hello"])
      end
    end

    context "with invalid queries" do
      it "raises an error for invalid syntax" do
        expect {
          MQ.run(".invalid_selector!!!", "# Heading", nil)
        }.to raise_error(RuntimeError, /Error evaluating query/)
      end
    end
  end

  describe ".html_to_markdown" do
    it "converts HTML to Markdown" do
      html_content = "<h1>Hello World</h1><p>This is a <strong>test</strong>.</p>"
      expected_markdown = "# Hello World\n\nThis is a **test**."
      markdown = MQ.html_to_markdown(html_content, nil)
      expect(markdown.strip).to eq(expected_markdown)
    end

    it "converts HTML with options" do
      html_content = "<html><head><title>Page Title</title></head><body><h1>Content</h1></body></html>"

      options = MQ::ConversionOptions.new
      options.use_title_as_h1 = true

      markdown = MQ.html_to_markdown(html_content, options)
      expect(markdown).to include("# Page Title")
    end
  end

  describe MQ::Result do
    let(:content) { "# Title\n\n## Section 1\n\n## Section 2" }
    let(:result) { MQ.run(".h2", content, nil) }

    describe "#text" do
      it "returns text representation" do
        expect(result.text).to eq("## Section 1\n## Section 2")
      end
    end

    describe "#values" do
      it "returns array of values" do
        expect(result.values).to eq(["## Section 1", "## Section 2"])
      end
    end

    describe "#length" do
      it "returns the number of values" do
        expect(result.length).to eq(3)
      end
    end

    describe "#[]" do
      it "accesses values by index" do
        expect(result[1]).to eq("## Section 1")
        expect(result[2]).to eq("## Section 2")
      end

      it "raises error for out of range index" do
        expect { result[10] }.to raise_error
      end
    end

    describe "#each" do
      it "iterates over values" do
        values = []
        result.each { |v| values << v }
        expect(values).to eq(["", "## Section 1", "## Section 2"])
      end
    end
  end

  describe "InputFormat constants" do
    it "defines all input format constants" do
      expect(MQ::InputFormat::MARKDOWN).to eq(0)
      expect(MQ::InputFormat::MDX).to eq(1)
      expect(MQ::InputFormat::TEXT).to eq(2)
      expect(MQ::InputFormat::HTML).to eq(3)
      expect(MQ::InputFormat::RAW).to eq(4)
      expect(MQ::InputFormat::NULL).to eq(5)
    end
  end

  describe "ListStyle constants" do
    it "defines all list style constants" do
      expect(MQ::ListStyle::DASH).to eq(0)
      expect(MQ::ListStyle::PLUS).to eq(1)
      expect(MQ::ListStyle::STAR).to eq(2)
    end
  end

  describe "TitleSurroundStyle constants" do
    it "defines all title surround style constants" do
      expect(MQ::TitleSurroundStyle::DOUBLE).to eq(0)
      expect(MQ::TitleSurroundStyle::SINGLE).to eq(1)
      expect(MQ::TitleSurroundStyle::PAREN).to eq(2)
    end
  end

  describe "UrlSurroundStyle constants" do
    it "defines all URL surround style constants" do
      expect(MQ::UrlSurroundStyle::ANGLE).to eq(0)
      expect(MQ::UrlSurroundStyle::NONE).to eq(1)
    end
  end

  describe MQ::Options do
    it "can be instantiated with default values" do
      options = MQ::Options.new
      expect(options.input_format).to be_nil
      expect(options.list_style).to be_nil
    end

    it "allows setting input format" do
      options = MQ::Options.new
      options.input_format = MQ::InputFormat::TEXT
      expect(options.input_format).to eq(MQ::InputFormat::TEXT)
    end

    it "allows setting list style" do
      options = MQ::Options.new
      options.list_style = MQ::ListStyle::PLUS
      expect(options.list_style).to eq(MQ::ListStyle::PLUS)
    end
  end

  describe MQ::ConversionOptions do
    it "can be instantiated with default values" do
      options = MQ::ConversionOptions.new
      expect(options.extract_scripts_as_code_blocks).to be false
      expect(options.generate_front_matter).to be false
      expect(options.use_title_as_h1).to be false
    end

    it "allows setting options" do
      options = MQ::ConversionOptions.new
      options.extract_scripts_as_code_blocks = true
      options.generate_front_matter = true
      options.use_title_as_h1 = true

      expect(options.extract_scripts_as_code_blocks).to be true
      expect(options.generate_front_matter).to be true
      expect(options.use_title_as_h1).to be true
    end
  end
end
