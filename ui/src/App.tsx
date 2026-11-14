import { LoginScreen } from "./screens/LoginScreen";
import { MainScreen } from "./screens/MainScreen";

export function App({ label }: { label: string }) {
  switch (label) {
    case "login":
      return <LoginScreen />;
    case "main":
      return <MainScreen />;
    default:
      return null;
  }
}
