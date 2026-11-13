/** The text of an error for the screen. */
export function errorText(error: unknown): string {
  if (typeof error === "string") return error;
  if (error && typeof error === "object" && "message" in error) {
    const message = (error as { message: unknown }).message;
    if (typeof message === "string" && message.length > 0) return message;
  }
  return "Something went wrong. Try again.";
}

/** The kind of an error, or "unknown". The provenance of an error is decided in native.ts, not here. */
export function errorKind(error: unknown): string {
  if (error && typeof error === "object" && "kind" in error) {
    return String((error as { kind: unknown }).kind);
  }
  return "unknown";
}
