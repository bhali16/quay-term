<script lang="ts">
  // The app icon, drawn inline so it stays crisp at any size.
  let { size = 64, id = "m" }: { size?: number; id?: string } = $props();
</script>

<svg width={size} height={size} viewBox="0 0 1024 1024" aria-hidden="true">
  <defs>
    <linearGradient id="{id}-bg" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="#27325c" />
      <stop offset="1" stop-color="#101530" />
    </linearGradient>
    <linearGradient id="{id}-fg" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0" stop-color="#5eead4" />
      <stop offset="1" stop-color="#38bdf8" />
    </linearGradient>
  </defs>
  <rect x="64" y="64" width="896" height="896" rx="200" fill="url(#{id}-bg)" />
  <path d="M290 330 L520 510 L290 690" fill="none" stroke="url(#{id}-fg)" stroke-width="84" stroke-linecap="round" stroke-linejoin="round" />
  <rect x="580" y="648" width="190" height="84" rx="42" fill="#38bdf8" />
  <path d="M170 830 q85 -44 171 0 t171 0 t171 0 t171 0" fill="none" stroke="#38bdf8" stroke-opacity="0.55" stroke-width="30" stroke-linecap="round" />
</svg>
