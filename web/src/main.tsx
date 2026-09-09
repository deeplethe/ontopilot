import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { RouterProvider } from "@tanstack/react-router";
import { router } from "./router";
import { ToastHost } from "./toast";
import { DEFAULT_STALE_MS, applyQueryDefaults } from "./queryDefaults";
import "./styles.css";

// staleTime 按键分档，表在 queryDefaults.ts；这里只给全局默认（#518）
const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      retry: false,
      refetchOnWindowFocus: false,
      staleTime: DEFAULT_STALE_MS,
    },
  },
});
applyQueryDefaults(queryClient);

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <RouterProvider router={router} />
      <ToastHost />
    </QueryClientProvider>
  </StrictMode>,
);
