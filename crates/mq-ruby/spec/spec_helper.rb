# frozen_string_literal: true

require "bundler/setup"

# Load the compiled extension
begin
  require "mq"
rescue LoadError
  # If the extension isn't built yet, provide a helpful message
  warn "WARNING: mq extension not loaded. Run 'rake compile' first."
end

RSpec.configure do |config|
  # Enable flags like --only-failures and --next-failure
  config.example_status_persistence_file_path = ".rspec_status"

  # Disable RSpec exposing methods globally on `Module` and `main`
  config.disable_monkey_patching!

  config.expect_with :rspec do |c|
    c.syntax = :expect
  end
end
