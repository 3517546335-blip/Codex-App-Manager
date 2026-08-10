import { useEffect, useRef, useState } from "react";

/** Smoothly animates a displayed number toward `target` (easeOutCubic), so live
 *  values — download %, bytes, speed — glide instead of snapping on every
 *  progress event. Honors prefers-reduced-motion by jumping straight to target.
 *  `fromRef` always holds the current on-screen value, so a new target mid-flight
 *  re-eases from wherever the number currently is (no visible jump). */
export function useCountUp(target: number, duration = 480): number {
  const [display, setDisplay] = useState(target);
  const fromRef = useRef(target);
  const rafRef = useRef(0);

  useEffect(() => {
    if (fromRef.current === target) return;
    if (window.matchMedia?.("(prefers-reduced-motion: reduce)").matches) {
      fromRef.current = target;
      setDisplay(target);
      return;
    }
    const from = fromRef.current;
    const start = performance.now();
    const tick = (now: number) => {
      const t = Math.min(1, (now - start) / duration);
      const eased = 1 - Math.pow(1 - t, 3);
      const cur = from + (target - from) * eased;
      fromRef.current = cur;
      setDisplay(cur);
      if (t < 1) {
        rafRef.current = requestAnimationFrame(tick);
      } else {
        fromRef.current = target;
      }
    };
    cancelAnimationFrame(rafRef.current);
    rafRef.current = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(rafRef.current);
  }, [target, duration]);

  return display;
}
