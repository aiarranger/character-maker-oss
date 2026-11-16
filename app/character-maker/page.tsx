import CharacterStudio from '../../components/character-maker/CharacterStudio';

// Character Studio の本体は components/character-maker/CharacterStudio.tsx（yukikau.jp などへの埋め込みと共通）。
// このページは同じ場所で魚群繚乱（/gyogun/）を配っているので、「魚群繚乱で遊ぶ」を出す（docs/gyogun-spec.md）。
export default function Page() {
  return <CharacterStudio gyogun />;
}
