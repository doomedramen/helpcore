import "@testing-library/jest-dom/vitest";

// cmdk (used by Command component) requires ResizeObserver in jsdom.
class ResizeObserverMock {
  observe() {}
  unobserve() {}
  disconnect() {}
}
window.ResizeObserver = ResizeObserverMock as unknown as typeof ResizeObserver;

// cmdk calls scrollIntoView on selected items.
Element.prototype.scrollIntoView = () => {};
