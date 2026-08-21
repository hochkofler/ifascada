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

// jsdom does not implement pointer capture or scrollIntoView; the vendored Radix-based
// Select needs both when a test actually opens the dropdown (its trigger/viewport call
// hasPointerCapture/setPointerCapture/releasePointerCapture and scrollIntoView on select).
if (!Element.prototype.hasPointerCapture) {
  Element.prototype.hasPointerCapture = () => false;
}
if (!Element.prototype.setPointerCapture) {
  Element.prototype.setPointerCapture = () => {};
}
if (!Element.prototype.releasePointerCapture) {
  Element.prototype.releasePointerCapture = () => {};
}
if (!Element.prototype.scrollIntoView) {
  Element.prototype.scrollIntoView = () => {};
}
