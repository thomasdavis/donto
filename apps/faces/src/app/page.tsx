import { FacesShell } from "@/components/FacesShell";

export default function Page() {
  // The shell is a client component; everything below is interactive.
  return <FacesShell dontosrvUrl={process.env.NEXT_PUBLIC_DONTOSRV_URL!} />;
}
