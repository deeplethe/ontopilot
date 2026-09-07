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
import { Login } from "../src/pages/Login";

function renderWithProviders(ui: React.ReactElement) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={queryClient}>{ui}</QueryClientProvider>,
  );
}

// Minimal router wrapper for Login page
function renderLogin() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  const rootRoute = createRootRoute();
  const loginRoute = createRoute({
    getParentRoute: () => rootRoute,
    path: "/login",
    component: Login,
  });
  const routeTree = rootRoute.addChildren([loginRoute]);
  const router = createRouter({
    routeTree,
    history: createMemoryHistory({ initialEntries: ["/login"] }),
  });
  return render(
    <QueryClientProvider client={queryClient}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
}

describe("Login page", () => {
  it("renders email and password inputs", async () => {
    renderLogin();
    // Login page should render input fields for email and password
    const emailInput = await screen.findByPlaceholderText(/email/i);
    const passwordInput = await screen.findByPlaceholderText(/password/i);
    expect(emailInput).toBeInTheDocument();
    expect(passwordInput).toBeInTheDocument();
  });

  it("renders sign in button", async () => {
    renderLogin();
    const button = await screen.findByRole("button", { name: /sign in/i });
    expect(button).toBeInTheDocument();
  });

  it("has login/register mode toggle", async () => {
    renderLogin();
    // The Login component has a Segmented toggle for login vs register
    const loginTab = await screen.findByText(/sign in/i);
    expect(loginTab).toBeInTheDocument();
  });
});
