/** DOM-only motion shared by the Vue console and React Skill manager. */
const reducedMotion = () => matchMedia('(prefers-reduced-motion: reduce)').matches;

export function createSlidingSelection(root: HTMLElement, horizontal = false) {
  let animation: Animation | undefined;
  let initialized = false;
  let destination = '';
  let disposed = false;
  function update(animate = false) {
    const target = root.querySelector<HTMLElement>("button.active, a.active, [aria-current='page']");
    const indicator = root.querySelector<HTMLElement>('.selection-indicator');
    if (disposed || !target || !indicator || !target.offsetWidth || !root.clientHeight) return;
    const transform = `translate(${target.offsetLeft}px, ${horizontal ? root.clientHeight - 3 : target.offsetTop}px)`;
    const width = `${target.offsetWidth}px`, height = horizontal ? '3px' : `${target.offsetHeight}px`;
    const next = `${transform}/${width}/${height}`;
    if (next === destination) return;
    const previous = getComputedStyle(indicator);
    const from = { transform: previous.transform, width: previous.width };
    animation?.cancel();
    Object.assign(indicator.style, { width, height, transform, opacity: '1' });
    if (animate && initialized && !reducedMotion()) {
      animation = indicator.animate(horizontal ? [from, { transform, width }] : [
        { transform: from.transform, offset: 0 },
        { transform: `${transform} scale(.96, 1.08)`, offset: .68 },
        { transform: `${transform} scale(1.015, .96)`, offset: .86 },
        { transform, offset: 1 },
      ], { duration: horizontal ? 280 : 460, easing: 'cubic-bezier(.22,.7,.22,1)' });
    }
    if (horizontal) {
      const left = target.offsetLeft, right = left + target.offsetWidth;
      if (left < root.scrollLeft) root.scrollTo({ left });
      else if (right > root.scrollLeft + root.clientWidth) root.scrollTo({ left: right - root.clientWidth });
    }
    initialized = true;
    destination = next;
  }
  const observer = new ResizeObserver(() => update());
  observer.observe(root);
  root.querySelectorAll('button, a').forEach(item => observer.observe(item));
  const frame = requestAnimationFrame(() => update());
  void document.fonts.ready.then(() => update());
  return { update, dispose() { disposed = true; observer.disconnect(); cancelAnimationFrame(frame); animation?.cancel(); } };
}

const panelAnimations = new WeakMap<Element, Animation>();
export function animateNavPanel(element: Element, entering: boolean) {
  panelAnimations.get(element)?.cancel();
  if (reducedMotion()) return;
  const animation = element.animate(entering ? [
    { opacity: 0, transform: 'translateY(8px)', offset: 0 },
    { opacity: 1, offset: 200 / 240 },
    { opacity: 1, transform: 'translateY(0)', offset: 1 },
  ] : [
    { opacity: 1, transform: 'translateY(0)' },
    { opacity: 0, transform: 'translateY(-4px)' },
  ], { duration: entering ? 240 : 100, easing: entering ? 'cubic-bezier(.2,.7,.2,1)' : 'ease', fill: 'both' });
  panelAnimations.set(element, animation);
  return animation;
}
export function cancelNavPanel(element: Element) { panelAnimations.get(element)?.cancel(); panelAnimations.delete(element); }
export function enterNavPanel(element: Element, done: () => void) {
  const animation = animateNavPanel(element, true);
  if (animation) void animation.finished.then(() => { animation.cancel(); done(); }, done);
  else queueMicrotask(done);
}
export function leaveNavPanel(element: Element, done: () => void) {
  const animation = animateNavPanel(element, false);
  if (animation) void animation.finished.then(done, done);
  // Vue must finish its current DOM patch before out-in can enter the next page.
  else queueMicrotask(done);
}
