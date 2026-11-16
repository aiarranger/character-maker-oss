// Studio で作ったキャラを、同じ場所で配っている魚群繚乱（/gyogun/）へ渡す。docs/gyogun-spec.md の「Studio で作ったキャラで遊ぶ」
// - キャラの ZIP（魚群繚乱用の .character.zip）は、ブラウザの控え（Cache Storage）の決まった 1 つのキーに置く。サーバーには送らない
// - 魚群繚乱（games/gyogun/rust/web/index.html）は、始めるときに同じキーを読む。置けるのは最後に送った 1 体
// - URL の from=studio は「Studio から来た直後」の印（魚群繚乱はそのキャラを選んだ状態で始める）

/** 控えの名前とキー（games/gyogun/rust/web/index.html と同じ値） */
export const GYOGUN_CACHE_NAME = 'character-maker-gyogun-v1';
export const GYOGUN_CHARACTER_KEY = '/gyogun/studio-character.zip';
/** 魚群繚乱のページ（pnpm gyogun:build が public/gyogun/ に作る） */
export const GYOGUN_PAGE = '/gyogun/index.html';
export const GYOGUN_PLAY_URL = `${GYOGUN_PAGE}?from=studio`;

export const GYOGUN_NOT_BUILT = '魚群繚乱がまだ作られていません。`pnpm gyogun:build` を実行してから、もう一度押してください。';
export const GYOGUN_NO_STORE = 'このブラウザでは、キャラクターを魚群繚乱へ渡す控え（Cache Storage）を使えません。';

type Fetch = (input: string, init?: { method?: string; cache?: 'no-store' }) => Promise<{ ok: boolean }>;
type CacheStore = { open(name: string): Promise<{ put(key: string, response: Response): Promise<void> }> };

/** 魚群繚乱が配られているか（public/gyogun/ が無いと 404。黙って 404 のページへ飛ばさない） */
export async function assertGyogunBuilt(fetcher: Fetch = (input, init) => fetch(input, init)): Promise<void> {
  let found = false;
  try {
    found = (await fetcher(GYOGUN_PAGE, { method: 'HEAD', cache: 'no-store' })).ok;
  } catch { found = false; }
  if (!found) throw new Error(GYOGUN_NOT_BUILT);
}

/** キャラの ZIP を控えに置く（前に送った 1 体は置き換わる） */
export async function storeGyogunCharacter(file: Blob, store: CacheStore | undefined = typeof caches === 'undefined' ? undefined : caches): Promise<void> {
  if (!store) throw new Error(GYOGUN_NO_STORE);
  const cache = await store.open(GYOGUN_CACHE_NAME);
  await cache.put(GYOGUN_CHARACTER_KEY, new Response(file, { headers: { 'content-type': 'application/zip' } }));
}
