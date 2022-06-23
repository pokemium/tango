import React from "react";
import { Trans, useTranslation } from "react-i18next";
import { TransitionGroup } from "react-transition-group";
import semver from "semver";

import HealingIcon from "@mui/icons-material/Healing";
import SyncIcon from "@mui/icons-material/Sync";
import Box from "@mui/material/Box";
import Button from "@mui/material/Button";
import CircularProgress from "@mui/material/CircularProgress";
import Collapse from "@mui/material/Collapse";
import Fab from "@mui/material/Fab";
import List from "@mui/material/List";
import ListItemButton from "@mui/material/ListItemButton";
import ListItemText from "@mui/material/ListItemText";
import Modal from "@mui/material/Modal";
import Slide from "@mui/material/Slide";
import Stack from "@mui/material/Stack";
import Tooltip from "@mui/material/Tooltip";
import Typography from "@mui/material/Typography";

import { PatchInfo } from "../../../patch";
import { usePatches } from "../PatchesContext";
import PatchInfoDialog from "../PatchInfoDialog";

export default function PatchesPane({ active }: { active: boolean }) {
  const { i18n } = useTranslation();

  const { patches, update, updating } = usePatches();

  const patchNames = Object.keys(patches);
  patchNames.sort();

  const [patchDialogPatch, setPatchDialogPatch] = React.useState<{
    info: PatchInfo;
    name: string;
  } | null>(null);
  const [patchDialogOpen, setPatchDialogOpen] = React.useState(false);

  const listFormatter = new Intl.ListFormat(i18n.resolvedLanguage, {
    style: "long",
    type: "conjunction",
  });

  return (
    <Box
      sx={{
        position: "relative",
        width: "100%",
        height: "100%",
        display: active ? "flex" : "none",
        overflow: "auto",
      }}
    >
      {patchNames.length > 0 ? (
        <>
          <List dense disablePadding sx={{ flexGrow: 1 }}>
            <TransitionGroup>
              {patchNames.map((patchName) => {
                const version = semver.maxSatisfying(
                  Object.keys(patches[patchName].versions),
                  "*"
                );

                return (
                  <Collapse key={`${patchName} ${version}`}>
                    <ListItemButton
                      onClick={() => {
                        setPatchDialogPatch({
                          name: patchName,
                          info: patches[patchName],
                        });
                        setPatchDialogOpen(true);
                      }}
                    >
                      <ListItemText
                        primary={
                          <>
                            {patches[patchName].title} <small>v{version}</small>
                          </>
                        }
                        primaryTypographyProps={{
                          sx: {
                            whiteSpace: "nowrap",
                            overflow: "hidden",
                            textOverflow: "ellipsis",
                          },
                        }}
                        secondary={
                          <Trans
                            i18nKey="patches:byline"
                            values={{
                              authors: listFormatter.format(
                                patches[patchName].authors.flatMap(({ name }) =>
                                  name != null ? [name] : []
                                )
                              ),
                            }}
                          />
                        }
                        secondaryTypographyProps={{
                          sx: {
                            whiteSpace: "nowrap",
                            overflow: "hidden",
                            textOverflow: "ellipsis",
                          },
                        }}
                      />
                    </ListItemButton>
                  </Collapse>
                );
              })}
            </TransitionGroup>
          </List>
          <Tooltip title={<Trans i18nKey="patches:update" />}>
            <Fab
              color="primary"
              sx={{
                position: "fixed",
                bottom: "16px",
                right: "16px",
                animation: updating ? "spin 2s linear infinite" : null,
                "@keyframes spin": {
                  "0%": {
                    transform: "rotate(360deg)",
                  },
                  "100%": {
                    transform: "rotate(0deg)",
                  },
                },
              }}
              disabled={updating}
              onClick={() => {
                update();
              }}
            >
              <SyncIcon />
            </Fab>
          </Tooltip>
        </>
      ) : (
        <Box
          flexGrow={1}
          display="flex"
          justifyContent="center"
          alignItems="center"
          sx={{ userSelect: "none", color: "text.disabled" }}
        >
          <Stack alignItems="center" spacing={1}>
            <HealingIcon sx={{ fontSize: "4rem" }} />
            <Typography variant="h6">
              <Trans i18nKey="patches:no-patches" />
            </Typography>
            <Button
              disabled={updating}
              startIcon={
                updating ? (
                  <CircularProgress color="inherit" size="1em" />
                ) : null
              }
              variant="contained"
              onClick={() => {
                update();
              }}
            >
              <Trans i18nKey="patches:update" />
            </Button>
          </Stack>
        </Box>
      )}
      <Modal
        open={patchDialogOpen}
        onClose={(_e, _reason) => {
          setPatchDialogOpen(false);
        }}
      >
        <Slide
          in={patchDialogOpen}
          direction="up"
          unmountOnExit
          onExited={() => {
            setPatchDialogPatch(null);
          }}
        >
          <Box>
            {patchDialogPatch != null ? (
              <PatchInfoDialog
                name={patchDialogPatch.name}
                patchInfo={patchDialogPatch.info}
                onClose={() => {
                  setPatchDialogOpen(false);
                }}
              />
            ) : null}
          </Box>
        </Slide>
      </Modal>
    </Box>
  );
}
