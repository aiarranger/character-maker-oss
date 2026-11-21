// Studio で作ったキャラを、同じ場所で配っているゲーム（魚群繚乱 /gyogun/・魚々娘 /gyogyo/）へ渡す。
// docs/gyogun-spec.md の「Studio で作ったキャラで遊ぶ」・docs/gyogyo-spec.md の「Studio で作ったキャラで走る」
// - キャラの ZIP（ゲーム用の .character.zip。どちらのゲームも同じ形）は、ブラウザの控え（Cache Storage）のゲームごとの決まった 1 つのキーに置く。サーバーには送らない
// - ゲーム（games/<id>/rust/web/index.html）は、始めるときに同じキーを読む。置けるのは最後に送った 1 体
// - URL の from=studio は「Studio から来た直後」の印（ゲームはそのキャラを選んだ状態で始める）

export type StudioGame = 'gyogun' | 'gyogyo';

export type GameHandoff = {
  /** 画面に出す名前 */
  name: string;
  /** 控えの名前とキー（games/<id>/rust/web/index.html と同じ値） */
  cacheName: string;
  characterKey: string;
  /** ゲームのページ（pnpm <id>:build が public/<id>/ に作る） */
  page: string;
  playUrl: string;
  /** ゲームが作られていないときの知らせ */
  notBuilt: string;
  noStore: string;
};

function game(id: StudioGame, name: string): GameHandoff {
  const page = `/${id}/index.html`;
  return {
    name, cacheName: `character-maker-${id}-v1`, characterKey: `/${id}/studio-character.zip`, page, playUrl: `${page}?from=studio`,
    notBuilt: `${name}がまだ作られていません。\`pnpm ${id}:build\` を実行してから、もう一度押してください。`,
    noStore: `このブラウザでは、キャラクターを${name}へ渡す控え（Cache Storage）を使えません。`,
  };
}

export const STUDIO_GAMES: Record<StudioGame, GameHandoff> = { gyogun: game('gyogun', '魚群繚乱'), gyogyo: game('gyogyo', '魚々娘') };

type Fetch = (input: string, init?: { method?: string; cache?: 'no-store' }) => Promise<{ ok: boolean }>;
type CacheStore = { open(name: string): Promise<{ put(key: string, response: Response): Promise<void> }> };

/** ゲームが配られているか（public/<id>/ が無いと 404。黙って 404 のページへ飛ばさない） */
export async function assertGameBuilt(id: StudioGame, fetcher: Fetch = (input, init) => fetch(input, init)): Promise<void> {
  const g = STUDIO_GAMES[id];
  let found = false;
  try {
    found = (await fetcher(g.page, { method: 'HEAD', cache: 'no-store' })).ok;
  } catch { found = false; }
  if (!found) throw new Error(g.notBuilt);
}

/** キャラの ZIP を控えに置く（前に送った 1 体は置き換わる） */
export async function storeGameCharacter(id: StudioGame, file: Blob, store: CacheStore | undefined = typeof caches === 'undefined' ? undefined : caches): Promise<void> {
  const g = STUDIO_GAMES[id];
  if (!store) throw new Error(g.noStore);
  const cache = await store.open(g.cacheName);
  await cache.put(g.characterKey, new Response(file, { headers: { 'content-type': 'application/zip' } }));
}
