fn main() {
    let markdown_content = "
# Product List

| Product | Category | Price | Stock |
|---------|----------|-------|-------|
| Laptop  | Electronics | $1200 | 45 |
| Monitor | Electronics | $350 | 28 |
| Chair   | Furniture | $150 | 73 |
| Desk    | Furniture | $200 | 14 |
| Keyboard | Accessories | $80 | 35 |

| Product | Category | Price | Stock |
|---------|----------|-------|-------|
| Mouse   | Accessories | $25 | 50 |
| Headphones | Electronics | $120 | 32 |
| Bookshelf | Furniture | $180 | 17 |
| USB Cable | Accessories | $12 | 89 |
| Coffee Maker | Appliances | $85 | 24 |
    ";
    let mut engine = mq_lang::DefaultEngine::default();
    engine.load_builtin_module();

    let code = ".[][]";
    println!(
        "{:?}",
        engine
            .eval(
                code,
                mq_lang::parse_markdown_input(markdown_content).unwrap().into_iter()
            )
            .unwrap()
    );
}
