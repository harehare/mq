# frozen_string_literal: true

require_relative "mq/version"

begin
  # Try to load the compiled extension
  RUBY_VERSION =~ /(\d+\.\d+)/
  require_relative "mq/#{Regexp.last_match(1)}/mq_ruby"
rescue LoadError
  require_relative "mq/mq_ruby"
end

module MQ
  class Error < StandardError; end

  # Options class for configuring mq queries
  class Options
    attr_accessor :input_format, :list_style, :link_title_style, :link_url_style

    def initialize
      @input_format = nil
      @list_style = nil
      @link_title_style = nil
      @link_url_style = nil
    end

    def to_h
      {
        input_format: @input_format,
        list_style: @list_style,
        link_title_style: @link_title_style,
        link_url_style: @link_url_style
      }.compact
    end
  end

  # Conversion options for HTML to Markdown conversion
  class ConversionOptions
    attr_accessor :extract_scripts_as_code_blocks, :generate_front_matter, :use_title_as_h1

    def initialize
      @extract_scripts_as_code_blocks = false
      @generate_front_matter = false
      @use_title_as_h1 = false
    end

    def to_h
      {
        extract_scripts_as_code_blocks: @extract_scripts_as_code_blocks,
        generate_front_matter: @generate_front_matter,
        use_title_as_h1: @use_title_as_h1
      }
    end
  end

  # List style constants
  module ListStyle
    DASH = 0
    PLUS = 1
    STAR = 2
  end

  # Title surround style constants
  module TitleSurroundStyle
    DOUBLE = 0
    SINGLE = 1
    PAREN = 2
  end

  # URL surround style constants
  module UrlSurroundStyle
    ANGLE = 0
    NONE = 1
  end

  class << self
    # Run an mq query on the provided content
    #
    # @param code [String] The mq query string
    # @param content [String] The markdown/HTML/text content to process
    # @param options [Options, nil] Optional configuration options
    # @return [Result] The query results
    def run(code, content, options = nil)
      options_hash = options&.to_h
      _run(code, content, options_hash)
    end

    # Convert HTML to Markdown
    #
    # @param content [String] The HTML content to convert
    # @param options [ConversionOptions, nil] Optional conversion options
    # @return [String] The converted Markdown
    def html_to_markdown(content, options = nil)
      options_hash = options&.to_h
      _html_to_markdown(content, options_hash)
    end
  end
end
