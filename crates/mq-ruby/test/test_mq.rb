require "test/unit"
require_relative "../lib/mq"

class TestMq < Test::Unit::TestCase
  def test_run
    result = Mq.run(".h", "# Hello", "markdown")
    assert_equal(["# Hello"], result)
  end
end
