import "@testing-library/jest-dom/vitest";

// jsdom does not implement matchMedia; the vendored Sidebar's useIsMobile
// hook calls it on mount, so any test rendering SidebarProvider needs a stub.
if (!window.matchMedia) {
  window.matchMedia = (query: string) =>
    ({
      matches: false,
      media: query,
      onchange: null,
      addListener: () => {},
      removeListener: () => {},
      addEventListener: () => {},
      removeEventListener: () => {},
      dispatchEvent: () => false,
    }) as unknown as MediaQueryList;
}
