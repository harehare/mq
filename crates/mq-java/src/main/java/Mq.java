public class Mq {
    static native String[] run(String code, String input, String inputFormat);

    static {
        System.loadLibrary("mq_java");
    }

    public static void main(String[] args) {
        if (args.length < 3) {
            System.out.println("Usage: java Mq <code> <input> <inputFormat>");
            return;
        }
        String[] result = run(args[0], args[1], args[2]);
        for (String s : result) {
            System.out.println(s);
        }
    }
}
