/** Returns whether a keyboard event should run the current mq query. */
export function isRunShortcut(event: Pick<KeyboardEvent, "ctrlKey" | "key" | "metaKey">): boolean {
  return (event.metaKey || event.ctrlKey) && event.key === "Enter";
}
