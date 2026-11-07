export const REGISTRATION_DEADLINE_MS = 3000;
export const NATIVE_CALL_DEADLINE_MS = 3000;
export const STATUS_HEALTH_LIMIT_MS = 9000;

export function withDeadline<T>(work: Promise<T>, milliseconds: number): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    const timer = setTimeout(() => reject({ kind: "timeout", message: "The native side did not answer in time." }), milliseconds);
    work.then(
      (value) => { clearTimeout(timer); resolve(value); },
      (reason: unknown) => { clearTimeout(timer); reject(reason); },
    );
  });
}
