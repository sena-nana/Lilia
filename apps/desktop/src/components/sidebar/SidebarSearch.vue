<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, ref, watch } from "vue";
import FileText from "@lucide/vue/dist/esm/icons/file-text.mjs";
import Search from "@lucide/vue/dist/esm/icons/search.mjs";
import X from "@lucide/vue/dist/esm/icons/x.mjs";
import {
  searchSessions,
  type SearchResult,
} from "../../services/sessionSearch";
import { measurePerfAsync } from "@lilia/ui/diagnostics";
import SearchDropdown from "@lilia/ui/components/SearchDropdown";

const props = defineProps<{
  modelValue?: boolean;
}>();

const emit = defineEmits<{
  select: [result: SearchResult];
  "update:modelValue": [value: boolean];
}>();

const active = computed({
  get: () => props.modelValue ?? false,
  set: (value) => emit("update:modelValue", value),
});
const query = ref("");
const results = ref<SearchResult[]>([]);
const inputRef = ref<{ focus: () => void } | null>(null);
const selectedIdx = ref(0);
const searching = ref(false);
let disposed = false;
let searchSeq = 0;

watch(results, () => {
  selectedIdx.value = 0;
});

watch(
  query,
  (value) => {
    void refreshResults(value);
  },
);

async function refreshResults(rawQuery: string) {
  if (disposed) return;
  const seq = ++searchSeq;
  const trimmed = rawQuery.trim();
  if (!trimmed) {
    results.value = [];
    searching.value = false;
    return;
  }
  searching.value = true;
  try {
    const next = await measurePerfAsync("sidebar.search.query", () =>
      searchSessions(trimmed),
    );
    if (disposed || seq !== searchSeq) return;
    results.value = next.slice(0, 12);
  } catch {
    if (disposed || seq !== searchSeq) return;
    results.value = [];
  } finally {
    if (!disposed && seq === searchSeq) searching.value = false;
  }
}

async function openSearch() {
  if (disposed) return;
  active.value = true;
  query.value = "";
  results.value = [];
  selectedIdx.value = 0;
  await nextTick();
  if (disposed) return;
  inputRef.value?.focus();
}

function closeSearch() {
  if (disposed) return;
  active.value = false;
  query.value = "";
  results.value = [];
  selectedIdx.value = 0;
}

function selectResult(result: SearchResult) {
  if (disposed) return;
  emit("select", result);
  closeSearch();
}

function onKeydown(event: KeyboardEvent) {
  if (event.key === "Escape") {
    event.preventDefault();
    closeSearch();
    return;
  }
  if (event.key === "ArrowDown") {
    event.preventDefault();
    if (results.value.length) {
      selectedIdx.value = (selectedIdx.value + 1) % results.value.length;
    }
  } else if (event.key === "ArrowUp") {
    event.preventDefault();
    if (results.value.length) {
      selectedIdx.value =
        (selectedIdx.value - 1 + results.value.length) % results.value.length;
    }
  } else if (event.key === "Enter") {
    event.preventDefault();
    const result = results.value[selectedIdx.value];
    if (result) selectResult(result);
  }
}

onBeforeUnmount(() => {
  disposed = true;
  searchSeq += 1;
});

</script>

<template>
  <template v-if="!active">
    <button
      type="button"
      class="sb-icon-action"
      data-agent-id="sidebar.search.open"
      title="搜索会话"
      aria-label="搜索会话"
      @click="openSearch"
    >
      <Search :size="15" aria-hidden="true" />
    </button>
  </template>

  <template v-else>
    <SearchDropdown
      ref="inputRef"
      v-model="query"
      class="sidebar-search-dropdown"
      placeholder="搜索会话…"
      :spellcheck="false"
      @keydown="onKeydown"
    >
      <template #leading>
        <Search :size="14" aria-hidden="true" class="search-dropdown__leading" />
      </template>
      <template #trailing>
        <button
          type="button"
          class="search-dropdown__action"
          data-agent-id="sidebar.search.close"
          title="关闭搜索 (Esc)"
          aria-label="关闭搜索"
          @click="closeSearch"
        >
          <X :size="13" aria-hidden="true" />
        </button>
      </template>

      <template #default="{ highlightRangeSegments }">
        <template v-if="results.length">
          <button
            v-for="(result, index) in results"
            :key="result.route"
            type="button"
            class="search-dropdown__item"
            :class="{ 'is-active': index === selectedIdx }"
            :data-agent-id="`sidebar.search.result.${result.route}`"
            role="option"
            :aria-selected="index === selectedIdx"
            @mouseenter="selectedIdx = index"
            @click="selectResult(result)"
          >
            <span class="search-dropdown__title">
              <template
                v-for="(segment, segmentIndex) in highlightRangeSegments(result.title, result.highlights)"
                :key="segmentIndex"
              >
                <mark v-if="segment.mark">{{ segment.text }}</mark>
                <template v-else>{{ segment.text }}</template>
              </template>
            </span>
            <span v-if="result.projectName" class="search-dropdown__scope">
              {{ result.projectName }}
            </span>
          </button>
        </template>
        <p v-else-if="searching" class="search-dropdown__hint">搜索中…</p>
        <p v-else-if="query.trim()" class="search-dropdown__empty">没有匹配</p>
        <p v-else class="search-dropdown__hint">
          <FileText :size="11" aria-hidden="true" />
          输入关键词
        </p>
      </template>
    </SearchDropdown>
  </template>
</template>
