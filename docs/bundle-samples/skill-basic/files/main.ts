export async function run(input) {
  return {
    text: String(input?.text ?? "").slice(0, 200)
  };
}
