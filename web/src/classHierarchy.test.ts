import { describe, expect, it, vi } from "vitest";

vi.hoisted(() => {
  for (const name of ["WebGLRenderingContext", "WebGL2RenderingContext"]) {
    Object.defineProperty(globalThis, name, {
      configurable: true,
      value: class WebGLRenderingContext {},
    });
  }
});

import {
  Children,
  isValidElement,
  type ReactElement,
  type ReactNode,
} from "react";
import { renderToStaticMarkup } from "react-dom/server";
import type { EntityTypeView } from "./api";
import { en } from "./i18n/en";
import { zh } from "./i18n/zh";
import { ClassDefinition } from "./pages/Ontology";

const ROOT_ID = "root";
const DIRECT_CHILD_ID = "direct-child";
const OTHER_ID = "other";
const GRANDCHILD_ID = "grandchild";

type TestElement = ReactElement<{
  children?: ReactNode;
  onClick?: () => void;
}>;

const elementChildren = (element: TestElement): TestElement[] =>
  Children.toArray(element.props.children).filter(isValidElement) as TestElement[];

const entityType = (id: string, parents: string[]): EntityTypeView => ({
  id,
  key: id,
  label: id,
  color: "#000000",
  shape: "circle",
  builtin: false,
  parents,
  disjoint: [],
  primary_parent: parents[0] ?? null,
  description: "",
  usage: 0,
});

describe("class hierarchy details", () => {
  it("renders navigable parent and subclass controls", () => {
    const selected = entityType(DIRECT_CHILD_ID, [ROOT_ID]);
    const onSelectClass = vi.fn();
    const definition = ClassDefinition({
      cls: selected,
      allTypes: [
        entityType(ROOT_ID, []),
        selected,
        entityType(GRANDCHILD_ID, [DIRECT_CHILD_ID]),
        entityType(OTHER_ID, []),
      ],
      onSelectClass,
      onNewSub: () => undefined,
    }) as TestElement;
    const html = renderToStaticMarkup(definition);

    expect(html).toContain(`>${ROOT_ID}</button>`);
    expect(html).toContain(`>${GRANDCHILD_ID}</button>`);
    expect(html).not.toContain(`>${OTHER_ID}</button>`);

    const [parentField, subclassField] = elementChildren(definition);
    const [parentLink] = elementChildren(elementChildren(parentField)[0]);
    const [subclassLink] = elementChildren(elementChildren(subclassField)[0]);
    parentLink.props.onClick?.();
    subclassLink.props.onClick?.();
    expect(onSelectClass.mock.calls).toEqual([[ROOT_ID], [GRANDCHILD_ID]]);

    const leafHtml = renderToStaticMarkup(
      ClassDefinition({
        cls: entityType(OTHER_ID, []),
        allTypes: [entityType(OTHER_ID, [])],
        onSelectClass,
        onNewSub: () => undefined,
      }),
    );
    expect(leafHtml).toContain(">None</div>");
  });

  it("explains that inheritance is shown on Definition in both locales", () => {
    expect(en.ontology.subclasses).toBe("Subclasses");
    expect(en.ontology.noSubclasses).toBe("None");
    expect(zh.ontology.subclasses).toBe("子类");
    expect(zh.ontology.noSubclasses).toBe("无");
    expect(en.ontology.schemaNoRelationships).toContain("Definition");
    expect(zh.ontology.schemaNoRelationships).toContain("定义");
  });
});
