FROM mcr.microsoft.com/playwright:v1.64.0-noble@sha256:06a9939e57531807f8d5fd76ce44b53165ffb7d7501d87ab10e285c20b1e971f
COPY --chmod=0755 fake-lpstat.py /opt/fake/bin/lpstat
COPY measure.py /opt/measure.py
COPY green-triangle-line.svg /opt/green-triangle-line.svg
ENTRYPOINT ["python3", "/opt/measure.py"]
