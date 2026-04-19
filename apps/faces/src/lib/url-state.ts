"use client";

import { usePathname, useRouter, useSearchParams } from "next/navigation";
import { useCallback, useMemo } from "react";

/**
 * Tiny URL-state hook. Backs a record of string-valued params with the
 * page's query string, so every meaningful piece of UI state is shareable
 * by copying the URL.
 *
 * Empty / null values are removed from the URL.
 *
 * Usage:
 *   const [params, setParams] = useUrlState({ subject: "ex:darnell-brooks" });
 *   setParams({ subject: "ex:other" });   // pushes ?subject=ex:other
 *   setParams({ filterContext: "" });     // removes the param
 */
export function useUrlState(defaults: Record<string, string>) {
  const router      = useRouter();
  const pathname    = usePathname();
  const searchParams = useSearchParams();

  const params = useMemo(() => {
    const out: Record<string, string> = { ...defaults };
    searchParams.forEach((v, k) => { out[k] = v; });
    return out;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [searchParams]);

  const setParams = useCallback((patch: Record<string, string | null | undefined>) => {
    const next = new URLSearchParams(searchParams.toString());
    for (const [k, v] of Object.entries(patch)) {
      if (v == null || v === "" || v === defaults[k]) next.delete(k);
      else next.set(k, v);
    }
    const qs = next.toString();
    router.replace(qs ? `${pathname}?${qs}` : pathname, { scroll: false });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [searchParams, pathname, router]);

  return [params, setParams] as const;
}
