/**
 * Stepping through a list of options by scrolling over it.
 *
 * Used by the harmony selector, the sort row and the filter rail: hovering any
 * of them and turning the wheel moves through the options, which is quicker
 * than aiming at a 40px target.
 *
 * Deltas are accumulated rather than acted on directly. A mouse wheel sends one
 * large `deltaY` per notch, but a trackpad sends a stream of small ones, and
 * treating each as a step makes a single swipe run through every option at once.
 */

/** How much accumulated delta makes one step. */
const NOTCH = 40;

/** The shortest gap between steps, so a flick cannot outrun the eye. */
const COOLDOWN_MS = 90;

/**
 * Call `step` with `+1` or `-1` as the wheel turns over `node`.
 *
 * Down and right are positive, matching how the wheel reads on a horizontal
 * row: scrolling down moves rightward through the options.
 */
export function onWheelStep(
  node: HTMLElement,
  step: (direction: 1 | -1) => void,
): void {
  let carry = 0;
  let last = 0;

  node.addEventListener(
    "wheel",
    (event: WheelEvent) => {
      // A horizontal swipe on a trackpad should work too, and whichever axis
      // the user moved is the one they meant.
      const delta =
        Math.abs(event.deltaX) > Math.abs(event.deltaY)
          ? event.deltaX
          : event.deltaY;
      if (delta === 0) return;

      // The row does not scroll, so the page must not scroll under it either.
      event.preventDefault();

      carry += delta;
      if (Math.abs(carry) < NOTCH) return;

      const now = performance.now();
      if (now - last < COOLDOWN_MS) {
        // Keep at most one notch of credit, or a fast flick banks a queue of
        // steps that keeps firing after the user has stopped.
        carry = Math.sign(carry) * Math.min(Math.abs(carry), NOTCH);
        return;
      }

      last = now;
      const direction = carry > 0 ? 1 : -1;
      carry = 0;
      step(direction);
    },
    // Not passive: this handler calls `preventDefault`.
    { passive: false },
  );
}

/**
 * Step through `length` options, wrapping at both ends.
 *
 * Wrapping matters for a wheel: there is no way to see that you have reached
 * the end, so stopping dead reads as the control having broken.
 */
export function stepIndex(
  current: number,
  direction: 1 | -1,
  length: number,
): number {
  if (length <= 0) return 0;
  return (current + direction + length) % length;
}
