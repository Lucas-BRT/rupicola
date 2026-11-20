# Diagnose CSS before adding rules

Use this workflow for layout, theming and component styling changes. It adapts ideas from the [CSS Demystified public curriculum](https://thecascade.dev/courses/css-demystified/curriculum/). Consult [modern CSS](modern-css.md) for feature support and fallbacks.

1. Reproduce the symptom with the actual DOM, content, theme and available space. Save a before capture and isolate the element and interaction involved.
2. Inspect computed styles and the winning declaration. Check the cascade layer, specificity, source order, inherited values and resolved custom properties. For overflow or alignment, inspect the containing block, available inline size, intrinsic minimums and track sizing. A long ID, native control or replaced element may impose the minimum.
3. Fix the rule that owns the constraint. Let normal flow size content. Use Flexbox for flow along one axis and Grid when rows and columns need shared alignment. Set a minimum track that fits its container. Use container queries for reusable components whose layout depends on allocated space and viewport queries for page structure. Use logical properties for directional layout.
4. Define component defaults and deliberate overrides. Use semantic custom properties where repeated controls or themes need a stable configuration point. Preserve default focus visibility, media aspect ratio and the order of interactive elements.
5. Recapture the same state, then test the boundary that caused the defect. Use a narrow container inside a wide viewport to distinguish component from viewport behavior. Exercise long labels, portrait and landscape media, zoom, keyboard focus and themes where relevant. Confirm the focused control stays visible and inspect the capture.

## Example

An image review surface compares screenshots in pairs. Preserve each pair as one DOM group. In a narrow review pane, stack the two screenshots while retaining their labels. In a wider pane, align them on equal tracks. Preserve their aspect ratios within the preview area and link to the full-size capture. Check both pane sizes with long checkpoint labels. An overflow measurement and a keyboard walkthrough supplement the screenshot.
