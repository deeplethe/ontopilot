import { describe, expect, it, vi } from "vitest";
import { en } from "../i18n/en";
import { zh } from "../i18n/zh";
import { decideMappings, selectedMappingIds } from "./Mappings";

describe("bulk mapping decisions", () => {
  it("decides every selected mapping exactly once", async () => {
    const decide = vi.fn(async (_id: string) => ({ ok: true }));

    await decideMappings(["mapping-a", "mapping-b", "mapping-c"], decide);

    expect(decide.mock.calls).toEqual([
      ["mapping-a"],
      ["mapping-b"],
      ["mapping-c"],
    ]);
  });

  it("waits for every decision before reporting a failure", async () => {
    const failure = new Error("mapping-a failed");
    let release!: () => void;
    const delayed = new Promise<void>((resolve) => {
      release = resolve;
    });
    const decide = vi.fn((id: string) =>
      id === "mapping-a" ? Promise.reject(failure) : delayed,
    );
    const batch = decideMappings(["mapping-a", "mapping-b"], decide);
    let settled = false;
    void batch.then(
      () => {
        settled = true;
      },
      () => {
        settled = true;
      },
    );

    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(settled).toBe(false);
    expect(decide.mock.calls).toEqual([["mapping-a"], ["mapping-b"]]);

    release();
    await expect(batch).rejects.toBe(failure);
  });

  it("never resubmits a selected mapping that is no longer proposed", () => {
    const picked = new Set(["mapping-a", "mapping-b"]);

    expect(
      selectedMappingIds(
        [
          { id: "mapping-a", status: "confirmed" },
          { id: "mapping-b", status: "proposed" },
          { id: "mapping-c", status: "proposed" },
        ],
        picked,
      ),
    ).toEqual(["mapping-b"]);
  });

  it("never submits a mapping with an individual decision in flight", () => {
    expect(
      selectedMappingIds(
        [{ id: "mapping-a", status: "proposed" }],
        new Set(["mapping-a"]),
        new Set(["mapping-a"]),
      ),
    ).toEqual([]);
  });

  it("names mapping checkboxes by concept and source in both locales", () => {
    expect(en.mapping.selectMapping("Revenue", "warehouse")).toBe(
      "Select Revenue from warehouse",
    );
    expect(zh.mapping.selectMapping("营收", "数据仓库")).toBe(
      "选择 数据仓库 中的 营收",
    );
    expect(en.mapping.selected(2)).toBe("2 selected");
    expect(zh.mapping.selected(2)).toBe("已选 2 条");
  });
});
