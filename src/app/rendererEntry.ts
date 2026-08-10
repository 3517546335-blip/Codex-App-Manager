import { installStaticCrashPolicy, renderStaticCrashFallback } from "./staticCrashFallback";

interface BootstrapModule {
  bootstrap: () => void | Promise<void>;
}

export type BootstrapLoader = () => Promise<BootstrapModule>;
export type BootstrapPolicyInstaller = () => () => void;

const loadBootstrap: BootstrapLoader = () => import("./bootstrap");
const installBootstrapPolicy: BootstrapPolicyInstaller = () => installStaticCrashPolicy();

/**
 * Dependency-light renderer entry. Keep React, ReactDOM and every provider in
 * the dynamically loaded bootstrap chunk so a failed chunk load or top-level
 * module evaluation still lands on the plain-DOM recovery surface.
 */
export async function startRenderer(
  loader: BootstrapLoader = loadBootstrap,
  installPolicy: BootstrapPolicyInstaller = installBootstrapPolicy,
): Promise<void> {
  const disposeBootstrapPolicy = installPolicy();
  try {
    const { bootstrap } = await loader();
    await bootstrap();
    // bootstrap installs the normal policy, which preserves editable context
    // menus. Drop the stricter startup guard only after that handoff succeeds.
    disposeBootstrapPolicy();
  } catch (cause) {
    const error = cause instanceof Error ? cause : new Error(String(cause));
    renderStaticCrashFallback(error);
  }
}
