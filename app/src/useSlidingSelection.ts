import { nextTick, watch, type Ref } from 'vue';
import { createSlidingSelection } from './navigationMotion';

export function useSlidingSelection(container: Ref<HTMLElement | null>, selection: Ref<string>, horizontal = false) {
  let controller: ReturnType<typeof createSlidingSelection> | undefined;
  watch(selection, () => void nextTick(() => controller?.update(true)), { flush: 'post' });
  watch(container, (root, _, cleanup) => {
    if (!root) return;
    controller = createSlidingSelection(root, horizontal);
    cleanup(() => { controller?.dispose(); controller = undefined; });
  }, { flush: 'post' });
}
