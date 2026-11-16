import { LoginScreen } from "./screens/LoginScreen";
import { MainScreen } from "./screens/MainScreen";
import { PlayerControlsScreen } from "./screens/PlayerControlsScreen";

export function App({ label }: { label: string }) {
  switch (label) {
    case "login":
      return <LoginScreen />;
    case "main":
      return <MainScreen />;
    case "player-controls":
      return <PlayerControlsScreen />;
    default:
      return null;
  }
}
