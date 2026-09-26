import { useEffect, useRef } from "react";

/**
 * Android's back button walks the webview history. Key Maker keeps one
 * sentinel history entry and routes every back press to the innermost screen
 * that registered a handler — so back means "previous screen", never "leave
 * the app with the vault open".
 */
const handlers: { current: () => void }[] = [];

export function installBackButton(): () => void {
  history.pushState({ keymaker: true }, "");
  const onPop = () => {
    history.pushState({ keymaker: true }, "");
    handlers[handlers.length - 1]?.current();
  };
  window.addEventListener("popstate", onPop);
  return () => window.removeEventListener("popstate", onPop);
}

export function useBackHandler(handler: (() => void) | undefined) {
  const ref = useRef(handler);
  ref.current = handler;
  const enabled = Boolean(handler);
  useEffect(() => {
    if (!enabled) return;
    const entry = { current: () => ref.current?.() };
    handlers.push(entry);
    return () => {
      const i = handlers.indexOf(entry);
      if (i >= 0) handlers.splice(i, 1);
    };
  }, [enabled]);
}
