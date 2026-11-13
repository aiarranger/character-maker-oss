// オリジナルの種族の部品の選び（three を読まない。下見の Rust の版も使う）。形と色を当てる処理は original-species.ts。
export type OriginalSpeciesParts = {
  head: 'original' | 'rounded' | 'wide' | 'slim' | 'cheeks';
  ears: 'original' | 'rounded' | 'folded';
  eyes: 'original' | 'large' | 'narrow';
  mouth: 'original' | 'small' | 'wide';
  coatColor: string; accentColor: string; eyeColor: string;
};
export function defaultOriginalSpeciesParts(): OriginalSpeciesParts {
  return { head: 'original', ears: 'original', eyes: 'original', mouth: 'original', coatColor: 'source', accentColor: 'source', eyeColor: 'source' };
}
const choices = <T extends string>(items: readonly (readonly [T, string])[]) => items.map(([id, label]) => ({ id, label }));
export function originalSpeciesChoices(characterId: string) {
  const robot = characterId === 'generic-robot', cat = characterId === 'generic-cat', human = ['generic-human-a', 'generic-human-b'].includes(characterId);
  return {
    head: choices(robot ? [['original', 'いつもの形'], ['rounded', 'まるい頭'], ['wide', 'ワイドな頭']] as const : cat ? [['original', 'いつもの顔'], ['slim', 'すっきり'], ['cheeks', 'ふっくらほっぺ']] as const : human ? [['original', 'いつもの顔'], ['rounded', 'まるみ'], ['slim', 'すっきり']] as const : [['original', '元の形']] as const),
    ears: choices(cat ? [['original', 'いつもの耳'], ['rounded', '丸みのある耳'], ['folded', 'たれ耳']] as const : [['original', '元の形']] as const),
    eyes: choices(robot || cat || human ? [['original', 'いつもの目'], ['large', '大きめ'], ['narrow', '横長']] as const : [['original', '元の形']] as const),
    mouth: choices(robot || cat || human ? [['original', 'いつもの口'], ['small', '小さめ'], ['wide', '横に広め']] as const : [['original', '元の形']] as const),
  };
}
export function validateOriginalSpeciesParts(value: unknown, characterId: string): OriginalSpeciesParts {
  if (value === undefined) return defaultOriginalSpeciesParts();
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error('顔と色の設定が正しくありません。');
  const input = value as Record<string, unknown>, allowed = originalSpeciesChoices(characterId), output = defaultOriginalSpeciesParts();
  for (const key of ['head', 'ears', 'eyes', 'mouth'] as const) {
    if (!allowed[key].some(item => item.id === input[key])) throw new Error('このキャラクターのパーツを選んでください。');
    Object.assign(output, { [key]: input[key] });
  }
  for (const key of ['coatColor', 'accentColor', 'eyeColor'] as const) {
    const color = input[key];
    if (typeof color !== 'string' || (color !== 'source' && !/^#[0-9a-f]{6}$/i.test(color))) throw new Error('パーツの色が正しくありません。');
    if (color !== 'source' && (!['generic-cat', 'generic-robot'].includes(characterId) || characterId === 'generic-robot' && key === 'eyeColor')) throw new Error('このキャラクターには、その色の設定がありません。');
    output[key] = color.toLowerCase();
  }
  return output;
}
