#!/usr/bin/env python3
"""Regenerate bilingual UML locally with an explicit PlantUML JAR; never part of site builds."""
import argparse
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parent.parent
# Stable aliases keep translations and exported graphs structurally identical.
NODES = {
    "FApp": ("flaccompagnon", "Desktop App", "App desktop"),
    "FCli": ("flaccompagnon-cli", "Command-line analysis", "Analyse en ligne de commande"),
    "FServices": ("flaccompagnon-services", "Tags / conversion / organization", "Tags / conversion / organisation"),
    "FCore": ("flaccompagnon-core", "Analysis / integrity / reports", "Analyse / intégrité / rapports"),
    "AApp": ("aede-desktop", "Server desktop console", "Console desktop du serveur"),
    "AConsole": ("aede-desktop-core", "Configuration / process lifecycle", "Configuration / cycle de vie"),
    "ACli": ("aede-cli", "Commands / server process", "Commandes / processus serveur"),
    "AServer": ("aede-server", "HTTP API / WebSocket", "API HTTP / WebSocket"),
    "ACore": ("aede-core", "Catalogue / credits / persistence", "Catalogue / crédits / persistance"),
    "AAccounts": ("aede-accounts", "Identities / roles / credentials", "Identités / rôles / accès"),
    "AMoods": ("aede-moods", "Moods / audio descriptors", "Ambiances / descripteurs audio"),
    "ADevices": ("aede-devices", "Network audio devices", "Lecteurs audio réseau"),
    "AAudio": ("aede-audio", "Decode / PCM queues / minimal output", "Décodage / files PCM / sortie minimale"),
    "ADsp": ("aede-dsp", "Audio signal processing", "Traitement du signal audio"),
    "PApp": ("phemios", "Music desktop App", "App desktop musicale"),
    "PCore": ("phemios-core", "Native playback orchestration", "Orchestration native de la lecture"),
    "PCd": ("phemios-cd", "Audio CDs / CUE images", "CD audio / images CUE"),
    "PTransfer": ("phemios-transfer", "Portable-device transfers", "Transferts vers les périphériques"),
}
EDGES = {
    "flaccompagnon": [("FApp", "FServices"), ("FApp", "FCore"), ("FCli", "FCore"), ("FServices", "FCore"), ("FApp", "AAudio", "output"), ("FCore", "AAudio", "decode")],
    "aede": [("AApp", "AConsole"), ("ACli", "AServer"), ("ACli", "ACore"), ("ACli", "ADevices"), ("ACli", "ADsp"), ("ACli", "AAudio", "queue"), ("ACli", "FCore"), ("AServer", "ACore"), ("AServer", "ADevices"), ("AServer", "AMoods"), ("AServer", "ADsp"), ("ADevices", "ACore"), ("ACore", "AAccounts"), ("ACore", "AMoods"), ("ACore", "ADsp"), ("ACore", "AAudio", "decode + dsp"), ("ACore", "FCore"), ("AAudio", "ADsp")],
    "phemios": [("PApp", "PCore"), ("PApp", "PCd"), ("PApp", "PTransfer"), ("PCore", "PCd"), ("PCore", "AAudio", "queue"), ("AAudio", "ADsp")],
}
STYLE = '''!pragma layout smetana
skinparam backgroundColor #15191D
skinparam defaultFontName SansSerif
skinparam defaultFontSize 16
skinparam defaultFontColor #EDF2EE
skinparam shadowing false
skinparam roundcorner 10
skinparam componentStyle uml2
skinparam ArrowColor #96A79F
skinparam ArrowFontColor #D0DDD5
skinparam ArrowFontSize 13
skinparam packageStyle rectangle
skinparam packageBackgroundColor #1E2528
skinparam packageBorderColor #52655D
skinparam packageFontColor #F1F5F2
skinparam packageFontSize 19
skinparam componentBackgroundColor #273035
skinparam componentBorderColor #6A7F73
skinparam componentFontColor #EDF2EE
skinparam nodesep 30
skinparam ranksep 35
hide stereotype
'''


def node(alias, language):
    name, english, french = NODES[alias]
    color = " #3B4930" if alias in ("FCore", "AAudio", "ADsp") else ""
    return f'component "{name}\\n{french if language == "fr" else english}" as {alias}{color}'


def diagram(kind, language):
    fr = language == "fr"
    title = ("Trois applications, des moteurs partagés" if fr else "Three applications, shared engines") if kind == "overview" else {"aede": "Aède", "flaccompagnon": "FlacCompagnon", "phemios": "Phémios", "complete": "Écosystème complet" if fr else "Complete ecosystem"}[kind]
    lines = [f"@startuml {language}-{kind}", STYLE, f"title {title}"]
    if kind == "overview":
        for alias, label in [("Flac", "FlacCompagnon\\nApp + flaccompagnon-cli"), ("Aede", "Aède\\nApp + aede-cli + API"), ("Phemios", "Phémios\\nApp")]:
            lines.append(f'component "{label}" as {alias}')
        lines += [node(alias, language) for alias in ("FCore", "AAudio", "ADsp")]
        lines += ["Flac -[hidden]right-> Aede", "Aede -[hidden]right-> Phemios", 'Flac -[#D8F36A]-> FCore : ' + ("analyse" if fr else "analysis"), 'Aede -[#D8F36A]-> FCore : ' + ("analyse" if fr else "analysis"), 'Flac -[#D8F36A]-> AAudio : ' + ("lecture minimale" if fr else "minimal playback"), 'Aede -[#D8F36A]-> AAudio : ' + ("décodage / PCM" if fr else "decoding / PCM"), 'Phemios -[#D8F36A]-> AAudio : ' + ("files PCM" if fr else "PCM queues"), "AAudio --> ADsp", 'Phemios -[#71CDE1,dashed]left-> Aede : HTTPS / WSS']
    else:
        groups = {
            "flaccompagnon": [("FlacCompagnon", ["FApp", "FCli", "FServices", "FCore"]), ("Aède · socle partagé" if fr else "Aède · shared foundation", ["AAudio", "ADsp"])],
            "aede": [("Console desktop" if fr else "Desktop console", ["AApp", "AConsole"]), ("Moteur Aède" if fr else "Aède engine", ["ACli", "AServer", "ADevices", "ACore", "AAccounts", "AMoods", "AAudio", "ADsp"]), ("FlacCompagnon", ["FCore"])],
            "phemios": [("Phémios", ["PApp", "PCore", "PCd", "PTransfer"]), ("Aède · socle partagé" if fr else "Aède · shared foundation", ["AAudio", "ADsp"])],
            "complete": [("FlacCompagnon", ["FApp", "FCli", "FServices", "FCore"]), ("Aède", ["AApp", "AConsole", "ACli", "AServer", "ADevices", "ACore", "AAccounts", "AMoods", "AAudio", "ADsp"]), ("Phémios", ["PApp", "PCore", "PCd", "PTransfer"])],
        }
        for label, aliases in groups[kind]:
            lines += [f'package "{label}" {{'] + [node(alias, language) for alias in aliases] + ["}"]
        if kind == "flaccompagnon":
            lines.append("AAudio --> ADsp : output")
        edges = list(dict.fromkeys(edge for application in EDGES.values() for edge in application)) if kind == "complete" else EDGES[kind]
        for edge in edges:
            source, target, *label = edge
            color = " -[#D8F36A]-> " if (source.startswith("F") != target.startswith("F") or source.startswith("P") != target.startswith("P")) else " --> "
            lines.append(source + color + target + (" : " + label[0] if label else ""))
        if kind in ("aede", "complete"):
            lines.append('AConsole -[#71CDE1,dashed]right-> ACli : ' + ("lance un processus" if fr else "starts a process"))
        if kind == "phemios":
            lines += ['component "aede-server\\nHTTPS / WSS" as AServer', "PCore -[#71CDE1,dashed]-> AServer : API", 'PApp -[#71CDE1,dashed]-> AServer : ' + ("via le pont natif" if fr else "via native bridge")]
        if kind == "complete":
            lines += ["PCore -[#71CDE1,dashed]-> AServer : HTTPS / WSS", 'PApp -[#71CDE1,dashed]-> AServer : ' + ("via le pont natif" if fr else "via native bridge")]
    lines += ["legend bottom", ("Flèche pleine : utilise la bibliothèque cible." if fr else "Solid arrow: uses the target library."), ("Pointillés bleus : connexion ou processus à l’exécution." if fr else "Blue dashed arrow: runtime connection or process."), "endlegend", "@enduml", ""]
    return "\n".join(lines)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--jar", required=True, type=Path)
    parser.add_argument("--java", default="java")
    args = parser.parse_args()
    sources = ROOT / "docs/diagrams/ecosystem"
    assets = ROOT / "site/assets/architecture"
    sources.mkdir(parents=True, exist_ok=True)
    assets.mkdir(parents=True, exist_ok=True)
    command = [args.java, "-Djava.awt.headless=true", "-DPLANTUML_SECURITY_PROFILE=SANDBOX", "-jar", str(args.jar.resolve())]
    for language in ("en", "fr"):
        for kind in ("overview", "aede", "flaccompagnon", "phemios", "complete"):
            source = sources / f"{language}-{kind}.puml"
            source.write_text(diagram(kind, language), encoding="utf-8")
            subprocess.run(command + ["--check-syntax", str(source)], check=True)
            for extension in ("svg", "png"):
                subprocess.run(command + ["--format", extension, "--output-dir", str(assets), str(source)], check=True)
    print("Ten bilingual diagrams validated and rendered locally.")


if __name__ == "__main__":
    main()
