import { describe, expect, it } from "vitest";
import { resolve } from "./theme";

describe("theme", () => {
  it("system follows the OS, the other two do not", () => {
    expect(resolve("system", "light")).toBe("light");
    expect(resolve("system", "dark")).toBe("dark");
    expect(resolve("light", "dark")).toBe("light");
    expect(resolve("dark", "light")).toBe("dark");
  });
});
