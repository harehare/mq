fn main() {
    let mut engine = mq_lang::Engine::default();
    engine.load_builtin_module();

    let code = r##"# Rule110 Cellular Automaton
# Rule110 is a cellular automaton with the following rules:
# 000 → 0, 001 → 1, 010 → 1, 011 → 1, 100 → 0, 101 → 1, 110 → 1, 111 → 0
def rule110(left, center, right):
  let pattern = s"${left}${center}${right}"
  | if (pattern == "000"): 0
  elif (pattern == "001"): 1
  elif (pattern == "010"): 1
  elif (pattern == "011"): 1
  elif (pattern == "100"): 0
  elif (pattern == "101"): 1
  elif (pattern == "110"): 1
  elif (pattern == "111"): 0
  else: 0;

def safe_get(arr, index):
  if (and(index >= 0, index < len(arr))):
    nth(arr, index)
  else:
    0;

def next_generation(current_gen):
  let width = len(current_gen)
  | map(range(0, width, 1),
  fn(i):
    let left = safe_get(current_gen, sub(i, 1))
    | let center = nth(current_gen, i)
    | let right = safe_get(current_gen, add(i, 1))
    | rule110(left, center, right);
);

def generation_to_string(gen):
  map(gen, fn(cell): if (cell == 1): "█" else: " ";) | join("");

def run_rule110(initial_state, generations):
  let result = [initial_state]
  | let i = 0
  | until (i < generations):
      let current_gen = last(result)
      | let next_gen = next_generation(current_gen)
      | let result = result + [next_gen]
      | let i = i + 1
      | result;;

let width = 81
| let initial_state = map(range(0, width, 1), fn(i): if (i == floor(div(width, 2))): 1 else: 0;)
| let generations = run_rule110(initial_state, 50)
| foreach (gen, generations):
    generation_to_string(gen);
| join("\n")
"##;
    println!(
        "{:?}",
        engine
            .eval(code, mq_lang::null_input().into_iter())
            .unwrap()
            .compact()
    );
}
