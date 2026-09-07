import { describe, it, expect, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
  RouterProvider,
  createMemoryHistory,
  createRootRoute,
  createRoute,
  createRouter,
} from "@tanstack/react-router";
import { Search } from "../src/pages/Search";

// Mock the kb context since Search depends on useKbId/useKb
vi.mock("../src/kb", () => ({
  useKbId: () => "test-kb-id",
  useKb: () => ({
    kb: {
      id: "test-kb-id",
      name: "Test KB",
      my_role: "editor",
    },
  }),
}));

function renderSearch(query?: string) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  const rootRoute = createRootRoute();
  const appRoute = createRoute({
    getParentRoute: () => rootRoute,
    path: "/app",
  });
  const kbRoute = createRoute({
    getParentRoute: () => appRoute,
    path: "/kb/$kbId",
  });
  const searchRoute = createRoute({
    getParentRoute: () => kbRoute,
    path: "/search",
    component: Search,
    validateSearch: (search: Record<string, unknown>) => ({
      q: (search.q as string) || "",
    }),
  });
  const routeTree = rootRoute.addChildren([
    appRoute.addChildren([kbRoute.addChildren([searchRoute])]),
  ]);
  const initialUrl = query
    ? `/app/kb/test-kb-id/search?q=${encodeURIComponent(query)}`
    : "/app/kb/test-kb-id/search";
  const router = createRouter({
    routeTree,
    history: createMemoryHistory({ initialEntries: [initialUrl] }),
  });
  return render(
    <QueryClientProvider client={queryClient}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
}

describe("Search page", () => {
  it("renders search input", async () => {
    renderSearch();
    const input = await screen.findByRole("textbox");
    expect(input).toBeInTheDocument();
  });
});
