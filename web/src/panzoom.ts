/// Pan/zoom for the rendered map. Transforms a content wrapper with a CSS
/// matrix (translate + scale) inside a clipped viewport. Zoom anchors on the
/// cursor; drag pans. `fit()` recenters and scales the content to the
/// viewport. Screen-space math keeps zoom-to-cursor exact regardless of the
/// SVG's own aspect ratio.

const MIN_SCALE = 0.2;
const MAX_SCALE = 40;
const ZOOM_STEP = 1.2;

export class PanZoom {
  private scale = 1;
  private tx = 0;
  private ty = 0;
  private dragging = false;
  private lastX = 0;
  private lastY = 0;
  private contentW = 0;
  private contentH = 0;

  constructor(
    private readonly viewport: HTMLElement,
    private readonly content: HTMLElement,
  ) {
    this.content.style.transformOrigin = "0 0";
    this.content.style.willChange = "transform";
    this.viewport.style.touchAction = "none";

    this.viewport.addEventListener("wheel", this.onWheel, { passive: false });
    this.viewport.addEventListener("pointerdown", this.onPointerDown);
    window.addEventListener("pointermove", this.onPointerMove);
    window.addEventListener("pointerup", this.onPointerUp);
    this.viewport.addEventListener("dblclick", () => this.fit());
  }

  /// Call after the content's natural size is known (e.g. SVG injected).
  setContentSize(w: number, h: number) {
    this.contentW = w;
    this.contentH = h;
  }

  /// Scale + center content to fill the viewport with a small margin.
  fit() {
    if (!this.contentW || !this.contentH) return;
    const vw = this.viewport.clientWidth;
    const vh = this.viewport.clientHeight;
    const margin = 0.96;
    this.scale = Math.min(vw / this.contentW, vh / this.contentH) * margin;
    this.tx = (vw - this.contentW * this.scale) / 2;
    this.ty = (vh - this.contentH * this.scale) / 2;
    this.apply();
  }

  zoomBy(factor: number) {
    const vw = this.viewport.clientWidth;
    const vh = this.viewport.clientHeight;
    this.zoomAt(vw / 2, vh / 2, factor);
  }

  /// Map a screen (client) point to content-space coordinates — i.e. SVG user
  /// units, which equal world coordinates offset by the SVG's viewBox origin.
  clientToContent(clientX: number, clientY: number): { x: number; y: number } {
    const rect = this.viewport.getBoundingClientRect();
    return {
      x: (clientX - rect.left - this.tx) / this.scale,
      y: (clientY - rect.top - this.ty) / this.scale,
    };
  }

  /// Animate the view to frame a content-space rectangle. Used for the
  /// coarse-first drill-in: zoom into the clicked region using the current
  /// (parent) pixels while the refined sector is generated, then the swap is
  /// seamless because both show the same area.
  focusContentRect(x: number, y: number, w: number, h: number) {
    const vw = this.viewport.clientWidth;
    const vh = this.viewport.clientHeight;
    const s = this.clampScale(Math.min(vw / w, vh / h) * 0.98);
    this.scale = s;
    this.tx = (vw - w * s) / 2 - x * s;
    this.ty = (vh - h * s) / 2 - y * s;
    this.content.style.transition = "transform 0.32s ease-out";
    this.apply();
    window.setTimeout(() => {
      this.content.style.transition = "";
    }, 360);
  }

  zoomIn() {
    this.zoomBy(ZOOM_STEP);
  }

  zoomOut() {
    this.zoomBy(1 / ZOOM_STEP);
  }

  private clampScale(s: number) {
    return Math.max(MIN_SCALE, Math.min(MAX_SCALE, s));
  }

  private zoomAt(px: number, py: number, factor: number) {
    const next = this.clampScale(this.scale * factor);
    const ratio = next / this.scale;
    // Keep the content point under (px,py) fixed during zoom.
    this.tx = px - (px - this.tx) * ratio;
    this.ty = py - (py - this.ty) * ratio;
    this.scale = next;
    this.apply();
  }

  private onWheel = (e: WheelEvent) => {
    e.preventDefault();
    const rect = this.viewport.getBoundingClientRect();
    const factor = e.deltaY < 0 ? ZOOM_STEP : 1 / ZOOM_STEP;
    this.zoomAt(e.clientX - rect.left, e.clientY - rect.top, factor);
  };

  private onPointerDown = (e: PointerEvent) => {
    this.dragging = true;
    this.lastX = e.clientX;
    this.lastY = e.clientY;
    this.viewport.classList.add("grabbing");
  };

  private onPointerMove = (e: PointerEvent) => {
    if (!this.dragging) return;
    this.tx += e.clientX - this.lastX;
    this.ty += e.clientY - this.lastY;
    this.lastX = e.clientX;
    this.lastY = e.clientY;
    this.apply();
  };

  private onPointerUp = () => {
    this.dragging = false;
    this.viewport.classList.remove("grabbing");
  };

  private apply() {
    this.content.style.transform = `translate(${this.tx}px, ${this.ty}px) scale(${this.scale})`;
  }
}
