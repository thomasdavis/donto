import { ExploreShell } from "@/components/ExploreShell";

export const metadata = { title: "donto · explore" };

export default function ExplorePage() {
  return <ExploreShell dontosrvUrl={process.env.NEXT_PUBLIC_DONTOSRV_URL!} />;
}
