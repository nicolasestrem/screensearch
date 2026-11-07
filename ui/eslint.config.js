// Flat ESLint config. The Rules-of-Hooks gate is error-level per the UI
// guardrails (`04`): all hooks before any early return, conditionals in JSX not
// around hooks. Generated bindings and build output are ignored.
//
// The token fence (`UI_REFERENCE` §2) is a local plugin object defined below, so
// the gate needs no extra dependency. It holds one rule per way a component
// could escape the v2 token layer: a raw colour literal, an rgb()/hsl()
// function, a px font size, an arbitrary font size or px value, a layer-1
// `--p-*` read, and a numeric px value inside an inline style. It inspects AST
// string literals, template chunks and style-object property values only, never
// raw file text, so prose in a comment (issue numbers such as #106) cannot trip
// it.
import js from "@eslint/js";
import globals from "globals";
import tseslint from "typescript-eslint";
import reactHooks from "eslint-plugin-react-hooks";
import reactRefresh from "eslint-plugin-react-refresh";

/** One rule per way a component could escape the v2 token layer. */
const TOKEN_RULES = [
  {
    id: "colour-literal",
    why: "a raw hex colour belongs in the --p- primitive layer, not in a component",
    pattern: /#[0-9a-fA-F]{3,8}\b/,
  },
  {
    id: "colour-function",
    why: "rgb()/rgba()/hsl()/hsla() bypass the semantic layer",
    pattern: /\b(?:rgba?|hsla?)\(/,
  },
  {
    id: "font-size-literal",
    why: "font sizes come from the type scale (text-t-*), never from a declaration",
    pattern: /font-size:\s*[0-9]/,
  },
  {
    id: "arbitrary-font-size",
    why: "arbitrary font sizes skip the 12px floor and the scale",
    pattern: /text-\[[0-9.]+(?:px|rem)\]/,
  },
  {
    id: "arbitrary-px",
    why: "arbitrary px values skip the density-aware spacing scale",
    // `text-[12px]` is reported by the font-size rule; this one is everything else.
    pattern: /(?<!text-)\[[0-9.]+px\]/,
  },
  {
    id: "primitive-token",
    why: "components speak semantic tokens; --p-* is layer 1 only",
    pattern: /var\(--p-/,
  },
];

const STYLE_RULE = {
  id: "inline-style-number",
  why: "an inline style with a numeric px value escapes the density scale",
};

/**
 * CSS properties whose bare number is a count, a ratio or a grid line, not a px
 * length (React's unitless set). `gridColumn: 1` and `opacity: 0.5` are honest
 * numbers; `marginTop: 7` is 7px and must come from the spacing scale.
 */
const UNITLESS_PROPERTIES = new Set([
  "animationIterationCount",
  "aspectRatio",
  "borderImageOutset",
  "borderImageSlice",
  "borderImageWidth",
  "boxFlex",
  "boxFlexGroup",
  "boxOrdinalGroup",
  "columnCount",
  "columns",
  "flex",
  "flexGrow",
  "flexPositive",
  "flexShrink",
  "flexNegative",
  "flexOrder",
  "fillOpacity",
  "floodOpacity",
  "fontWeight",
  "gridArea",
  "gridColumn",
  "gridColumnEnd",
  "gridColumnSpan",
  "gridColumnStart",
  "gridRow",
  "gridRowEnd",
  "gridRowSpan",
  "gridRowStart",
  "lineClamp",
  "lineHeight",
  "opacity",
  "order",
  "orphans",
  "stopOpacity",
  "strokeDasharray",
  "strokeDashoffset",
  "strokeMiterlimit",
  "strokeOpacity",
  "strokeWidth",
  "tabSize",
  "widows",
  "zIndex",
  "zoom",
]);

/** A px measurement written as a string or a template chunk. */
const PX_VALUE = /[0-9]+(?:\.[0-9]+)?px/;

const tokenPlugin = {
  rules: {
    "no-raw-token": {
      meta: {
        type: "problem",
        docs: { description: "Reject styling that bypasses the v2 token layer" },
        schema: [],
        messages: { violation: "{{why}} ({{rule}})" },
      },
      create(context) {
        const report = (node, rule) =>
          context.report({ node, messageId: "violation", data: { rule: rule.id, why: rule.why } });

        /** Every rule that matches this literal text is reported. */
        const checkText = (node, value) => {
          if (typeof value !== "string") return;
          for (const rule of TOKEN_RULES) {
            if (rule.pattern.test(value)) report(node, rule);
          }
        };

        const propertyName = (property) => {
          if (property.key.type === "Identifier") return property.key.name;
          if (property.key.type === "Literal" && typeof property.key.value === "string") {
            return property.key.value;
          }
          return null;
        };

        const checkStyleObject = (object) => {
          for (const property of object.properties) {
            if (property.type !== "Property") continue; // spreads cannot be judged statically
            const name = propertyName(property);
            const value = property.value;
            const isFontSize = name === "fontSize" || name === "font-size";
            const pxRule = isFontSize
              ? TOKEN_RULES.find((rule) => rule.id === "font-size-literal")
              : STYLE_RULE;
            const isPxLength = name !== null && !UNITLESS_PROPERTIES.has(name);

            if (value.type === "Literal") {
              if (typeof value.value === "number") {
                if (isFontSize || isPxLength) report(property, pxRule);
              } else if (typeof value.value === "string" && PX_VALUE.test(value.value)) {
                report(property, isFontSize ? pxRule : STYLE_RULE);
              }
              // String literals are already checked by the Literal visitor above.
            } else if (value.type === "TemplateLiteral") {
              const text = value.quasis.map((quasi) => quasi.value.cooked ?? quasi.value.raw).join("");
              if (PX_VALUE.test(text)) report(property, isFontSize ? pxRule : STYLE_RULE);
            }
          }
        };

        return {
          Literal(node) {
            const { value } = node;
            checkText(node, typeof value === "string" ? value : undefined);
          },
          TemplateElement(node) {
            checkText(node, node.value.cooked ?? node.value.raw);
          },
          JSXAttribute(node) {
            if (node.name.type !== "JSXIdentifier" || node.name.name !== "style") return;
            const expression = node.value?.expression;
            if (expression?.type === "ObjectExpression") checkStyleObject(expression);
          },
        };
      },
    },
  },
};

export default tseslint.config(
  { ignores: ["dist", "src/bindings"] },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  {
    files: ["**/*.{ts,tsx}"],
    languageOptions: {
      ecmaVersion: 2021,
      globals: globals.browser,
    },
    plugins: {
      "react-hooks": reactHooks,
      "react-refresh": reactRefresh,
    },
    rules: {
      ...reactHooks.configs.recommended.rules,
      "react-hooks/rules-of-hooks": "error",
      "react-refresh/only-export-components": ["warn", { allowConstantExport: true }],
    },
  },
  {
    // Every hand-written source file is in scope: the token files are the only
    // place a literal may live, and the generated bindings are not ours to fix.
    files: ["src/**/*.{ts,tsx}"],
    ignores: ["src/styles/**"],
    plugins: { tokens: tokenPlugin },
    rules: { "tokens/no-raw-token": "error" },
  },
);
