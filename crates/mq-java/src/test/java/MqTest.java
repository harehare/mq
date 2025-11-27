import org.junit.Test;
import static org.junit.Assert.*;

public class MqTest {
    @Test
    public void testRun() {
        String[] result = Mq.run(".h", "# Hello", "markdown");
        assertArrayEquals(new String[]{"# Hello"}, result);
    }
}
